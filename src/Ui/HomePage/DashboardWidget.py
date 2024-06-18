# -*- coding: utf-8 -*-
from PySide6.QtCore import Qt, QSize
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout
from qfluentwidgets import ToolButton, FluentIcon

from src.Ui.common.InfoCard import (
    NapCatVersionCard, QQVersionCard, CPUDashboard, MemoryDashboard, SystemInfoCard,
    BotListCard
)


class DashboardWidget(QWidget):

    def __init__(self, parent=None):
        super().__init__(parent=parent)

        # 创建布局和控件
        self.vBoxLayout = QVBoxLayout()
        self.hBoxLayout = QHBoxLayout()
        self.versionLayout = QVBoxLayout()
        self.buttonLayout = QVBoxLayout()
        self.infoLayout = QHBoxLayout()

        self.ncCard = NapCatVersionCard(self)
        self.qqCard = QQVersionCard(self)
        self.cpuCard = CPUDashboard(self)
        self.memoryCard = MemoryDashboard(self)
        self.documentButton = ToolButton(FluentIcon.DOCUMENT, self)
        self.reposButton = ToolButton(FluentIcon.GITHUB, self)
        self.feedbackButton = ToolButton(FluentIcon.HELP, self)
        self.systemInfoCard = SystemInfoCard(self)
        self.botList = BotListCard(self)

        # 设置控件
        self.documentButton.setFixedSize(60, 60)
        self.reposButton.setFixedSize(60, 60)
        self.feedbackButton.setFixedSize(60, 60)
        self.documentButton.setIconSize(QSize(22, 22))
        self.reposButton.setIconSize(QSize(22, 22))
        self.feedbackButton.setIconSize(QSize(22, 22))

        # 调用方法
        self._setLayout()

    def _setLayout(self):
        """
        ## 设置布局
        """
        self.versionLayout.setSpacing(0)
        self.versionLayout.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.versionLayout.addWidget(self.ncCard)
        self.versionLayout.addWidget(self.qqCard)
        self.versionLayout.setContentsMargins(0, 0, 0, 0)

        self.buttonLayout.setSpacing(10)
        self.buttonLayout.addSpacing(10)
        self.buttonLayout.addWidget(self.documentButton)
        self.buttonLayout.addWidget(self.reposButton)
        self.buttonLayout.addWidget(self.feedbackButton)
        self.buttonLayout.setContentsMargins(0, 0, 0, 0)

        self.hBoxLayout.setSpacing(0)
        self.hBoxLayout.addLayout(self.versionLayout)
        self.hBoxLayout.addWidget(self.cpuCard)
        self.hBoxLayout.addWidget(self.memoryCard)
        self.hBoxLayout.addLayout(self.buttonLayout)
        self.hBoxLayout.addStretch(1)

        self.infoLayout.addWidget(self.systemInfoCard)
        self.infoLayout.addSpacing(4)
        self.infoLayout.addWidget(self.botList)
        self.infoLayout.setContentsMargins(0, 0, 8, 0)

        self.vBoxLayout.addLayout(self.hBoxLayout)
        self.vBoxLayout.addSpacing(4)
        self.vBoxLayout.addLayout(self.infoLayout)
        self.vBoxLayout.setContentsMargins(0, 0, 0, 0)

        self.setLayout(self.vBoxLayout)


