# -*- coding: utf-8 -*-
from PySide6.QtWidgets import QWidget, QVBoxLayout

# 项目内模块导入
from src.Ui.StyleSheet import StyleSheet
from src.Ui.HomePage.ContentView.ContentTopCard import ContentTopCard
from src.Ui.HomePage.ContentView.DashboardWidget import DashboardWidget


class ContentViewWidget(QWidget):

    def __init__(self) -> None:
        """
        初始化
        """

        super().__init__()
        # 创建布局和控件
        self.vBoxLayout = QVBoxLayout(self)
        self.topCard = ContentTopCard(self)
        self.dashboardWidget = DashboardWidget(self)

        # 设置控件
        self.setObjectName("content_view")

        # 调用方法
        self._setLayout()

        # 应用样式表
        StyleSheet.HOME_WIDGET.apply(self)

    def _setLayout(self) -> None:
        """
        ## 对内部进行布局
        """
        self.vBoxLayout.addWidget(self.topCard)
        self.vBoxLayout.addWidget(self.dashboardWidget)
        self.vBoxLayout.setContentsMargins(24, 20, 24, 10)
        self.setLayout(self.vBoxLayout)
