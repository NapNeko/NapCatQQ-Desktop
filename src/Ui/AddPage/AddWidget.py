# -*- coding: utf-8 -*-
# 标准库导入
from abc import ABC
from typing import TYPE_CHECKING, Self, Optional

# 第三方库导入
from qfluentwidgets import isDarkTheme
from PySide6.QtWidgets import QWidget, QVBoxLayout

# 项目内模块导入
from src.Core.Config import cfg
from src.Ui.StyleSheet import StyleSheet
from src.Ui.common.widget import BackgroundWidget
from src.Core.Utils.singleton import singleton
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
    vBoxLayout: Optional[QVBoxLayout]

    def __init__(self) -> None:
        """
        ## AddWidget 继承自 QWidget 用于显示 侧边栏 AddBot 对应的 Widget

        ## 占位符初始化
            - view : 窗体内的切换页面控件(QStackedWidget)
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

    def _setLayout(self) -> None:
        """
        ## 布局内部控件
        """
        self.vBoxLayout.addWidget(self.view)
        self.vBoxLayout.setContentsMargins(24, 20, 24, 10)
        self.setLayout(self.vBoxLayout)
