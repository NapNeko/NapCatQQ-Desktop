# -*- coding: utf-8 -*-
from PySide6.QtWidgets import QWidget, QVBoxLayout, QStackedWidget

from src.Ui.HomePage.DashboardWidget import DashboardWidget
from src.Ui.StyleSheet import StyleSheet
from src.Ui.HomePage.ContentTopCard import ContentTopCard
from src.Ui.common.InfoCard import (
    QQVersionCard, NapCatVersionCard, CPUDashboard, MemoryDashboard
)


class ContentViewWidget(QWidget):

    def __init__(self) -> None:
        """
        初始化
        """

        super().__init__()
        # 创建布局和控件
        self.vBoxLayout = QVBoxLayout(self)
        self.topCard = ContentTopCard(self)
        self.view = QStackedWidget(self)
        self.dashboardWidget = DashboardWidget(self.view)

        # 设置控件
        self.setObjectName("content_view")
        self.view.addWidget(self.dashboardWidget)
        self.view.setCurrentWidget(self.dashboardWidget)

        # 调用方法
        self._setLayout()

        # 应用样式表
        StyleSheet.HOME_WIDGET.apply(self)

    def _setLayout(self):
        """
        ## 对内部进行布局
        """
        self.vBoxLayout.addWidget(self.topCard)
        self.vBoxLayout.addWidget(self.view)
        self.vBoxLayout.setContentsMargins(24, 20, 24, 10)
        self.setLayout(self.vBoxLayout)
