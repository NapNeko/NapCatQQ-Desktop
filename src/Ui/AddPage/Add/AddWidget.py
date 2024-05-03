# -*- coding: utf-8 -*-

"""
添加机器人
"""
from abc import ABC
from typing import TYPE_CHECKING, Self

from PySide6.QtWidgets import QVBoxLayout, QStackedWidget
from creart import add_creator, exists_module
from creart.creator import AbstractCreator, CreateTargetInfo
from qfluentwidgets import ScrollArea
from qfluentwidgets.components import ExpandLayout, SettingCardGroup

from src.Ui.AddPage.Add.ConfigTopCard import ConfigTopCard
from src.Ui.AddPage.Add.BotWidget import BotWidget
from src.Ui.AddPage.Add.Connect import ConnectWidget
from src.Ui.AddPage.Add.Advanced import AdvancedWidget
from src.Ui.StyleSheet import StyleSheet

if TYPE_CHECKING:
    from src.Ui.MainWindow import MainWindow


class AddWidget(ScrollArea):
    """
    ## 窗体中 Add Bot 对应的 Widget
    """

    def __init__(self):
        super().__init__()
        self.topCard: ConfigTopCard = None
        self.view: QStackedWidget = None

    def initialize(self, parent: "MainWindow") -> Self:
        """
        ## 初始化 Widget 所需要的控件并进行配置
        """
        # 创建控件
        self.topCard = ConfigTopCard(self)
        self._createView()

        # 设置 ScrollArea
        self.setParent(parent)
        self.setObjectName("AddPage")
        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.setViewportMargins(0, self.topCard.height(), 0, 0)
        self.view.setObjectName("AddView")

        # 应用样式表
        StyleSheet.ADD_WIDGET.apply(self)

        return self

    def _createView(self) -> None:
        """
        ## 创建并配置 QStackedWidget
        :return:
        """
        self.view = QStackedWidget()
        self.botWidget = BotWidget(self)
        self.connectWidget = ConnectWidget(self)
        self.advancedWidget = AdvancedWidget(self)

        self.view.addWidget(self.botWidget)
        self.view.addWidget(self.connectWidget)
        self.view.addWidget(self.advancedWidget)

        self.topCard.pivot.addItem(
            routeKey=self.botWidget.objectName(),
            text=self.tr("Bot"),
            onClick=lambda: self.view.setCurrentWidget(self.botWidget)
        )
        self.topCard.pivot.addItem(
            routeKey=self.connectWidget.objectName(),
            text=self.tr("Connect"),
            onClick=lambda: self.view.setCurrentWidget(self.connectWidget)
        )
        self.topCard.pivot.addItem(
            routeKey=self.advancedWidget.objectName(),
            text=self.tr("Advanced"),
            onClick=lambda: self.view.setCurrentWidget(self.advancedWidget)
        )

        # 连接信号并初始化当前标签页
        self.view.currentChanged.connect(self.onCurrentIndexChanged)
        self.view.setCurrentWidget(self.botWidget)
        self.topCard.pivot.setCurrentItem(self.botWidget.objectName())

    def getConfig(self) -> dict:
        """
        ## 返回配置结果
        """
        return {}

    def onCurrentIndexChanged(self, index):
        widget = self.view.widget(index)
        self.topCard.pivot.setCurrentItem(widget.objectName())

    def resizeEvent(self, event) -> None:
        """
        ## 重写缩放事件对 topCard 进行大小调整
        """
        super().resizeEvent(event)
        self.topCard.resize(self.width(), self.topCard.height())


class AddWidgetClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.Ui.AddPage.Add.AddWidget", "AddWidget"),)

    # 静态方法available()，用于检查模块"Add"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.Ui.AddPage.Add.AddWidget")

    # 静态方法create()，用于创建AddWidget类的实例，返回值为AddWidget对象。
    @staticmethod
    def create(create_type: [AddWidget]) -> AddWidget:
        return AddWidget()


add_creator(AddWidgetClassCreator)
