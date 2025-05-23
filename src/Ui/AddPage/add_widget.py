# -*- coding: utf-8 -*-
# 标准库导入
from abc import ABC
from typing import TYPE_CHECKING, Self, Optional

# 第三方库导入
from qfluentwidgets import isDarkTheme
from PySide6.QtCore import Slot
from PySide6.QtWidgets import QWidget, QVBoxLayout

# 项目内模块导入
from src.Core.Config import cfg
from src.Ui.StyleSheet import StyleSheet
from src.Ui.AddPage.enum import ConnectType
from src.Ui.common.widget import BackgroundWidget
from src.Ui.AddPage.connect import ConnectWidget
from src.Ui.AddPage.msg_box import (
    ChooseConfigTypeDialog,
    HttpClientConfigDialog,
    HttpServerConfigDialog,
    HttpSSEServerConfigDialog,
    WebsocketClientConfigDialog,
    WebsocketServerConfigDialog,
)
from src.Ui.AddPage.advanced import AdvancedWidget
from src.Core.Utils.singleton import singleton
from src.Ui.AddPage.bot_widget import BotWidget
from src.Ui.AddPage.signal_bus import addPageSingalBus
from src.Ui.AddPage.add_top_card import AddTopCard
from src.Ui.common.stacked_widget import TransparentStackedWidget

if TYPE_CHECKING:
    # 项目内模块导入
    from src.Ui.MainWindow import MainWindow


@singleton
class AddWidget(BackgroundWidget):
    """
    ## 窗体中 Add Bot 对应的 Widget
    """

    view: Optional[TransparentStackedWidget]
    topCard: Optional[AddTopCard]
    vBoxLayout: Optional[QVBoxLayout]

    def __init__(self) -> None:
        """
        ## AddWidget 继承自 QWidget 用于显示 侧边栏 AddBot 对应的 Widget

        ## 占位符初始化
            - view : 窗体内的切换页面控件(QStackedWidget)
            - topCard : 窗体内顶部操作控件(AddTopCard)
            - vBoxLayout : 窗体内的总布局
        """
        super().__init__()
        # 传入配置
        self.bgEnabledConfig = cfg.bgAddPage
        self.bgPixmapLightConfig = cfg.bgAddPageLight
        self.bgPixmapDarkConfig = cfg.bgAddPageDark
        self.bgOpacityConfig = cfg.bgAddPageOpacity

        # 调用方法
        super().updateBgImage()

    def initialize(self, parent: "MainWindow") -> Self:
        """
        ## 初始化 AddWidget 所需要的控件并进行配置
        """
        # 创建控件
        self.vBoxLayout = QVBoxLayout(self)
        self.topCard = AddTopCard(self)
        self._createView()

        # 设置 AddWidget
        self.setParent(parent)
        self.setObjectName("AddPage")
        self.view.setObjectName("AddView")

        # 调用方法
        self._setLayout()
        self._connectSignal()

        # 应用样式表
        StyleSheet.ADD_WIDGET.apply(self)

        return self

    def _createView(self) -> None:
        """
        ## 创建并配置 QStackedWidget
        """
        self.view = TransparentStackedWidget()
        self.botWidget = BotWidget(self)
        self.connectWidget = ConnectWidget(self)
        self.advancedWidget = AdvancedWidget(self)

        self.view.addWidget(self.botWidget)
        self.view.addWidget(self.connectWidget)
        self.view.addWidget(self.advancedWidget)

        self.topCard.pivot.addItem(
            routeKey=self.botWidget.objectName(),
            text=self.tr("基本配置"),
            onClick=lambda: self.view.setCurrentWidget(self.botWidget),
        )
        self.topCard.pivot.addItem(
            routeKey=self.connectWidget.objectName(),
            text=self.tr("连接配置"),
            onClick=lambda: self.view.setCurrentWidget(self.connectWidget),
        )
        self.topCard.pivot.addItem(
            routeKey=self.advancedWidget.objectName(),
            text=self.tr("高级配置"),
            onClick=lambda: self.view.setCurrentWidget(self.advancedWidget),
        )

        # 连接信号并初始化当前标签页
        self.view.currentChanged.connect(self.onCurrentIndexChanged)
        self.view.currentChanged.connect(addPageSingalBus.addWidgetViewChange.emit)
        self.view.setCurrentWidget(self.botWidget)
        self.topCard.pivot.setCurrentItem(self.botWidget.objectName())

    def _setLayout(self) -> None:
        """
        ## 布局内部控件
        """
        self.vBoxLayout.addWidget(self.topCard)
        self.vBoxLayout.addWidget(self.view)
        self.vBoxLayout.setContentsMargins(24, 20, 24, 10)
        self.setLayout(self.vBoxLayout)

    def _connectSignal(self) -> None:
        """
        ## 连接信号
        """
        # 连接信号
        addPageSingalBus.addConnectConfigButtonClicked.connect(self._onAddConnectConfigButtonClicked)
        addPageSingalBus.chooseConnectType.connect(self._onShowTypeDialog)

    @Slot()
    def _onAddConnectConfigButtonClicked(self) -> None:
        """添加连接配置按钮的槽函数"""
        # 项目内模块导入
        from src.Ui.MainWindow import MainWindow

        ChooseConfigTypeDialog(MainWindow()).exec()

    @Slot()
    def _onShowTypeDialog(self, connectType: ConnectType) -> None:
        """添加连接配置按钮的槽函数"""
        # 项目内模块导入
        from src.Ui.MainWindow import MainWindow

        if (
            dialog := {
                ConnectType.HTTP_SERVER: HttpServerConfigDialog,
                ConnectType.HTTP_SSE_SERVER: HttpSSEServerConfigDialog,
                ConnectType.HTTP_CLIENT: HttpClientConfigDialog,
                ConnectType.WEBSOCKET_SERVER: WebsocketServerConfigDialog,
                ConnectType.WEBSOCKET_CLIENT: WebsocketClientConfigDialog,
            }.get(connectType)(MainWindow())
        ).exec():
            self.connectWidget.addCard(dialog.getConfig())

    def getConfig(self) -> dict:
        """
        ## 返回配置结果
        """
        return {
            "bot": self.botWidget.getValue(),
            "connect": self.connectWidget.getValue(),
            "advanced": self.advancedWidget.getValue(),
        }

    def onCurrentIndexChanged(self, index) -> None:
        """
        ## 切换 Pivot 和 view 的槽函数
        """
        widget = self.view.widget(index)
        self.topCard.pivot.setCurrentItem(widget.objectName())
