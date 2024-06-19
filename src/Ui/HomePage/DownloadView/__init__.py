# -*- coding: utf-8 -*-
from PySide6.QtWidgets import QWidget, QVBoxLayout, QStackedWidget

from src.Ui.HomePage.DownloadView.DownloadTopCard import DownloadTopCard
from src.Ui.StyleSheet import StyleSheet

from src.Ui.common.Netwrok import NapCatDownloadCard, QQDownloadCard


class DownloadViewWidget(QWidget):

    def __init__(self) -> None:
        """
        初始化
        """
        super().__init__()
        # 设置控件
        self.setObjectName("download_view")

        # 创建布局和控件
        self.vBoxLayout = QVBoxLayout(self)
        self.topCard = DownloadTopCard(self)
        self.napcatCard = NapCatDownloadCard()
        self.qqCard = QQDownloadCard()

        # 调用方法
        self._setLayout()

        # 应用样式表
        StyleSheet.HOME_WIDGET.apply(self)

    def _setLayout(self):
        """
        ## 对内部进行布局
        """
        self.vBoxLayout.addWidget(self.topCard)
        self.vBoxLayout.addWidget(self.napcatCard)
        self.vBoxLayout.addWidget(self.qqCard)
        self.vBoxLayout.addStretch(1)
        self.vBoxLayout.setContentsMargins(24, 20, 24, 10)
        self.setLayout(self.vBoxLayout)