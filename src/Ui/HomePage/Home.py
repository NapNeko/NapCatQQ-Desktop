# -*- coding: utf-8 -*-
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
from src.Core.Utils.singleton import singleton
from src.Ui.HomePage.ContentView import ContentViewWidget
from src.Ui.HomePage.DisplayView import DisplayViewWidget
from src.Ui.common.stacked_widget import BackgroundStackedWidget

if TYPE_CHECKING:
    # 项目内模块导入
    from src.Ui.MainWindow import MainWindow


@singleton
class HomeWidget(BackgroundStackedWidget):
    displayView: Optional[DisplayViewWidget]

    def __init__(self) -> None:
        super().__init__()

        # 传入配置
        self.enabledDefaultBg = True
        self.bgDefaultLight = ":Global/image/Global/page_bg_light.png"
        self.bgDefaultDark = ":Global/image/Global/page_bg_dark.png"

        self.bgEnabledConfig = cfg.bgHomePage
        self.bgPixmapLightConfig = cfg.bgHomePageLight
        self.bgPixmapDarkConfig = cfg.bgHomePageDark
        self.bgOpacityConfig = cfg.bgHomePageOpacity

        # 调用方法
        super().updateBgImage()

    def initialize(self, parent: "MainWindow") -> Self:
        """
        初始化
        """
        # 创建控件
        self.displayView = DisplayViewWidget()

        # 设置控件
        self.setParent(parent)
        self.setObjectName("HomePage")
        self.addWidget(self.displayView)
        self.setCurrentWidget(self.displayView)

        # 链接信号
        self.displayView.buttonGroup.goButton.clicked.connect(lambda: self.parent().setCurrentIndex(1))

        # 调用方法
        self.updateBgImage()

        # 应用样式表
        StyleSheet.HOME_WIDGET.apply(self)

        return self
