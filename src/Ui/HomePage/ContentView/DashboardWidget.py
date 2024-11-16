# -*- coding: utf-8 -*-
# 第三方库导入
from qfluentwidgets import FluentIcon, ToolButton, ToolTipFilter
from PySide6.QtGui import QDesktopServices
from PySide6.QtCore import Qt, Slot, QSize
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout

# 项目内模块导入
from src.Ui.common.InfoCard import BotListCard, CPUDashboard, QQVersionCard, MemoryDashboard, NapCatVersionCard
from src.Core.NetworkFunc.Urls import Urls
from src.Ui.common.message_box import HyperlinkBox


class DashboardWidget(QWidget):

    def __init__(self, parent=None) -> None:
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
        self.botList = BotListCard(self)

        # 设置控件
        self.documentButton.setFixedSize(60, 60)
        self.reposButton.setFixedSize(60, 60)
        self.feedbackButton.setFixedSize(60, 60)
        self.documentButton.setIconSize(QSize(22, 22))
        self.reposButton.setIconSize(QSize(22, 22))
        self.feedbackButton.setIconSize(QSize(22, 22))

        # 连接信号
        self.documentButton.clicked.connect(lambda: QDesktopServices.openUrl(Urls.NAPCATQQ_DOCUMENT.value))
        self.reposButton.clicked.connect(self._showSelectReposMsgBox)
        self.feedbackButton.clicked.connect(self._showSelectFeedbackMsgBox)

        # 调用方法
        self._setToolTips()
        self._setLayout()

    def _setToolTips(self) -> None:
        """
        ## 设置工具提示
        """
        self.documentButton.installEventFilter(ToolTipFilter(self.documentButton))
        self.reposButton.installEventFilter(ToolTipFilter(self.reposButton))
        self.feedbackButton.installEventFilter(ToolTipFilter(self.feedbackButton))

        self.documentButton.setToolTip(self.tr("Click to open the NapCat document"))
        self.reposButton.setToolTip(self.tr("Click to open the repository page"))
        self.feedbackButton.setToolTip(self.tr("Click Submit feedback"))

    def _setLayout(self) -> None:
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

        self.infoLayout.addWidget(self.botList)
        self.infoLayout.setContentsMargins(0, 0, 8, 0)

        self.vBoxLayout.addLayout(self.hBoxLayout)
        self.vBoxLayout.addSpacing(4)
        self.vBoxLayout.addLayout(self.infoLayout)
        self.vBoxLayout.setContentsMargins(0, 0, 0, 0)

        self.setLayout(self.vBoxLayout)

    @Slot()
    def _showSelectReposMsgBox(self) -> None:
        """显示选择打开哪个仓库页面"""
        # 项目内模块导入
        from src.Ui.MainWindow.Window import MainWindow

        HyperlinkBox(
            self.tr("查看仓库"),
            self.tr("您可以点击下面的超链接按钮跳转到相应的仓库✨"),
            [
                {"name": self.tr("NapCatQQ 仓库"), "url": Urls.NAPCATQQ_REPO.value},
                {"name": self.tr("NapCatQQ Desktop 仓库"), "url": Urls.NCD_REPO.value},
            ],
            MainWindow(),
        ).exec()

    @Slot()
    def _showSelectFeedbackMsgBox(self) -> None:
        """显示选择打开哪个仓库的 issue 页面"""
        # 项目内模块导入
        from src.Ui.MainWindow.Window import MainWindow

        HyperlinkBox(
            self.tr("反馈渠道"),
            self.tr("请确定您要反馈的问题✨"),
            [
                {"name": self.tr("NapCatQQ 的问题"), "url": Urls.NAPCATQQ_ISSUES.value},
                {"name": self.tr("NapCatQQ Desktop 的问题"), "url": Urls.NCD_ISSUES.value},
            ],
            MainWindow(),
        ).exec()
