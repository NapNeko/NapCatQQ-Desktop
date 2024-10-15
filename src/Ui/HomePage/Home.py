# -*- coding: utf-8 -*-

"""
## 主页
"""
# 标准库导入
from abc import ABC
from typing import TYPE_CHECKING, Self, Optional

# 第三方库导入
from creart import add_creator, exists_module
from creart.creator import AbstractCreator, CreateTargetInfo
from qfluentwidgets import isDarkTheme
from PySide6.QtGui import QPixmap, QRegion, QPainter, QPainterPath
from PySide6.QtCore import Qt, QPoint, QRectF
from PySide6.QtWidgets import QStackedWidget

# 项目内模块导入
from src.Core.Config import cfg
from src.Ui.StyleSheet import StyleSheet
from src.Ui.HomePage.ContentView import ContentViewWidget
from src.Ui.HomePage.DisplayView import DisplayViewWidget
from src.Ui.common.stacked_widget import BackgroundStackedWidget

if TYPE_CHECKING:
    # 项目内模块导入
    from src.Ui.MainWindow import MainWindow


class HomeWidget(BackgroundStackedWidget):

    def __init__(self) -> None:
        super().__init__()
        self.displayView: Optional[DisplayViewWidget] = None
        # self.contentView: Optional[ContentViewWidget] = None

        self.enabledDefaultBg = True
        self.bgDefaultLight = ":Global/image/Global/page_bg_light.png"
        self.bgDefaultDark = ":Global/image/Global/page_bg_dark.png"

        self.bgEnabledConfig = cfg.bgHomePage
        self.bgPixmapLightConfig = cfg.bgHomePageLight
        self.bgPixmapDarkConfig = cfg.bgHomePageDark
        self.bgOpacityConfig = cfg.bgHomePageOpacity

        self.updateBgImage()

    def initialize(self, parent: "MainWindow") -> Self:
        """
        初始化
        """
        # 创建控件
        self.displayView = DisplayViewWidget()
        # self.contentView = ContentViewWidget()

        # 设置控件
        self.setParent(parent)
        self.setObjectName("HomePage")
        self.addWidget(self.displayView)
        # self.addWidget(self.contentView)
        self.setCurrentWidget(self.displayView)

        # 链接信号
        self.displayView.buttonGroup.goButton.clicked.connect(lambda: self.parent().setCurrentIndex(1))

        # 调用方法
        self.updateBgImage()

        # 应用样式表
        StyleSheet.HOME_WIDGET.apply(self)

        return self


class HomeWidgetClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.Ui.HomePage.Home", "HomeWidget"),)

    # 静态方法available()，用于检查模块"HomeWidget"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.Ui.HomePage.Home")

    # 静态方法create()，用于创建HomeWidget类的实例，返回值为HomeWidget对象。
    @staticmethod
    def create(create_type: [HomeWidget]) -> HomeWidget:
        return HomeWidget()


add_creator(HomeWidgetClassCreator)
