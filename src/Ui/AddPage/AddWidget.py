# -*- coding: utf-8 -*-
# 标准库导入
from abc import ABC
from typing import TYPE_CHECKING, Self, Optional

# 第三方库导入
from creart import add_creator, exists_module
from creart.creator import AbstractCreator, CreateTargetInfo
from qfluentwidgets import isDarkTheme
from PySide6.QtGui import QPixmap, QRegion, QPainter, QPainterPath
from PySide6.QtCore import Qt, QRectF
from PySide6.QtWidgets import QWidget, QVBoxLayout, QStackedWidget

# 项目内模块导入
from src.Core.Config import cfg
from src.Ui.StyleSheet import StyleSheet
from src.Ui.common.widget import BackgroundWidget
from src.Ui.AddPage.Connect import ConnectWidget
from src.Ui.AddPage.Advanced import AdvancedWidget
from src.Core.Utils.singleton import singleton
from src.Ui.AddPage.BotWidget import BotWidget
from src.Ui.AddPage.AddTopCard import AddTopCard
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