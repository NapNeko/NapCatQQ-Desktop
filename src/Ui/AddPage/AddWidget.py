# -*- coding: utf-8 -*-

"""
添加机器人
"""
from abc import ABC
from typing import TYPE_CHECKING, Self, Optional

from PySide6.QtWidgets import QStackedWidget, QWidget, QVBoxLayout
from creart import add_creator, exists_module, it
from creart.creator import AbstractCreator, CreateTargetInfo

from src.Ui.AddPage.Advanced import AdvancedWidget
from src.Ui.AddPage.BotWidget import BotWidget
from src.Ui.AddPage.Connect import ConnectWidget

from src.Ui.AddPage.ConfigTopCard import ConfigTopCard
from src.Ui.StyleSheet import StyleSheet

if TYPE_CHECKING:
    from src.Ui.MainWindow import MainWindow


class AddWidget(QWidget):
    """
    ## 窗体中 Add Bot 对应的 Widget
    """

    def __init__(self):
        super().__init__()
        self.view: Optional[QStackedWidget] = None
        self.topCard: Optional[ConfigTopCard] = None
        self.vBoxLayout: Optional[QVBoxLayout] = None

    def initialize(self, parent: "MainWindow") -> Self:
        """
        ## 初始化 Widget 所需要的控件并进行配置
        """
        # 创建控件
        self.vBoxLayout = QVBoxLayout(self)
        self.topCard = ConfigTopCard(self)
        self._createView()

        # 设置 QWidget
        self.setParent(parent)
        self.setObjectName("AddPage")
        self.view.setObjectName("AddView")

        # 调用方法
        self._setLayout()

        # 应用样式表
        StyleSheet.ADD_WIDGET.apply(self)

        return self

    def _createView(self) -> None:
        """
        ## 创建并配置 QStackedWidget
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
            onClick=lambda: self.view.setCurrentWidget(self.botWidget),
        )
        self.topCard.pivot.addItem(
            routeKey=self.connectWidget.objectName(),
            text=self.tr("Connect"),
            onClick=lambda: self.view.setCurrentWidget(self.connectWidget),
        )
        self.topCard.pivot.addItem(
            routeKey=self.advancedWidget.objectName(),
            text=self.tr("Advanced"),
            onClick=lambda: self.view.setCurrentWidget(self.advancedWidget),
        )

        # 连接信号并初始化当前标签页
        self.view.currentChanged.connect(self.onCurrentIndexChanged)
        self.view.setCurrentWidget(self.botWidget)
        self.topCard.pivot.setCurrentItem(self.botWidget.objectName())

    def _setLayout(self):
        """
        ## 布局内部控件
        """
        self.vBoxLayout.addWidget(self.topCard)
        self.vBoxLayout.addWidget(self.view)
        self.vBoxLayout.setContentsMargins(24, 20, 24, 10)
        self.setLayout(self.vBoxLayout)

    def getConfig(self) -> dict:
        """
        ## 返回配置结果
        """
        return {
            "bot": self.botWidget.getValue(),
            "connect": self.connectWidget.getValue(),
            "advanced": self.advancedWidget.getValue(),
        }

    def onCurrentIndexChanged(self, index):
        """
        ## 切换 Pivot 和 view 的槽函数
        """
        widget = self.view.widget(index)
        self.topCard.pivot.setCurrentItem(widget.objectName())

    def showInfo(self, title: str, content: str):
        """
        # 配置 InfoBar 的一些配置, 简化内部使用 InfoBar 的步骤
        """
        from src.Ui.MainWindow.Window import MainWindow
        it(MainWindow).showInfo(
            title=title,
            content=content,
            showcasePage=self
        )

    def showError(self, title: str, content: str):
        """
        # 配置 InfoBar 的一些配置, 简化内部使用 InfoBar 的步骤
        """
        from src.Ui.MainWindow.Window import MainWindow
        it(MainWindow).showError(
            title=title,
            content=content,
            showcasePage=self
        )

    def showWarning(self, title: str, content: str):
        """
        # 配置 InfoBar 的一些配置, 简化内部使用 InfoBar 的步骤
        """
        from src.Ui.MainWindow.Window import MainWindow
        it(MainWindow).showWarning(
            title=title,
            content=content,
            showcasePage=self
        )

    def showSuccess(self, title: str, content: str):
        """
        # 配置 InfoBar 的一些配置, 简化内部使用 InfoBar 的步骤
        """
        from src.Ui.MainWindow.Window import MainWindow
        it(MainWindow).showSuccess(
            title=title,
            content=content,
            showcasePage=self
        )


class AddWidgetClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.Ui.AddPage.AddWidget", "AddWidget"),)

    # 静态方法available()，用于检查模块"Add"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.Ui.AddPage.AddWidget")

    # 静态方法create()，用于创建AddWidget类的实例，返回值为AddWidget对象。
    @staticmethod
    def create(create_type: [AddWidget]) -> AddWidget:
        return AddWidget()


add_creator(AddWidgetClassCreator)
