# -*- coding: utf-8 -*-
from PySide6.QtCore import Qt, QSize
from PySide6.QtGui import QDesktopServices
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout
from qfluentwidgets import ToolButton, FluentIcon, ToolTipFilter, MessageBoxBase, TitleLabel, BodyLabel, HyperlinkButton

from src.Core.NetworkFunc import Urls
from src.Ui.common.InfoCard import (
    NapCatVersionCard, QQVersionCard, CPUDashboard, MemoryDashboard, SystemInfoCard,
    BotListCard
)


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
        self.systemInfoCard = SystemInfoCard(self)
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
        self.reposButton.clicked.connect(lambda: SelectReposMsgBox(self.parent().parent()).exec())
        self.feedbackButton.clicked.connect(lambda: SelectFeedbackMsgBox(self.parent().parent()).exec())

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

        self.infoLayout.addWidget(self.systemInfoCard)
        self.infoLayout.addSpacing(4)
        self.infoLayout.addWidget(self.botList)
        self.infoLayout.setContentsMargins(0, 0, 8, 0)

        self.vBoxLayout.addLayout(self.hBoxLayout)
        self.vBoxLayout.addSpacing(4)
        self.vBoxLayout.addLayout(self.infoLayout)
        self.vBoxLayout.setContentsMargins(0, 0, 0, 0)

        self.setLayout(self.vBoxLayout)


class SelectReposMsgBox(MessageBoxBase):
    """
    ## 让用户选择打开哪个仓库
    """

    def __init__(self, parent=None) -> None:
        super().__init__(parent=parent)
        # 创建标签
        self.titleLabel = TitleLabel(self.tr("Please select ..."), self)
        self.contentsLabel = BodyLabel(
            self.tr("You can click the hyperlink button below to jump to the\ncorresponding warehouse"), self
        )

        self.napcatReposButton = HyperlinkButton(
            url=Urls.NAPCATQQ_REPO.value.toString(),
            text=self.tr("NapCatQQ Repo"),
            icon=FluentIcon.LINK,
            parent=self
        )
        self.NCDReposButton = HyperlinkButton(
            url=Urls.NCD_REPO.value.toString(),
            text=self.tr("NapCatQQ Desktop Repo"),
            icon=FluentIcon.LINK,
            parent=self
        )

        # 添加到布局
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addWidget(self.contentsLabel)
        self.viewLayout.addWidget(self.napcatReposButton, 0, Qt.AlignmentFlag.AlignLeft)
        self.viewLayout.addWidget(self.NCDReposButton, 0, Qt.AlignmentFlag.AlignLeft)

        # 设置对话框
        self.widget.setMinimumWidth(300)
        self.cancelButton.hide()


class SelectFeedbackMsgBox(MessageBoxBase):
    """
    ## 让用户选择打开哪个仓库
    """

    def __init__(self, parent=None) -> None:
        super().__init__(parent=parent)
        # 创建标签
        self.titleLabel = TitleLabel(self.tr("Please select ..."), self)
        self.contentsLabel = BodyLabel(
            self.tr(
                "Is the problem you having with Desktop or NapCatQQ?\n"
                "Please go to the corresponding repository to raise issues"
            ),
            self
        )

        self.napcatIssuesButton = HyperlinkButton(
            url=Urls.NAPCATQQ_ISSUES.value.toString(),
            text=self.tr("NapCatQQ Issues"),
            icon=FluentIcon.LINK,
            parent=self
        )
        self.NCDIssuesButton = HyperlinkButton(
            url=Urls.NCD_ISSUES.value.toString(),
            text=self.tr("NapCatQQ Desktop Issues"),
            icon=FluentIcon.LINK,
            parent=self
        )

        # 添加到布局
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addWidget(self.contentsLabel)
        self.viewLayout.addWidget(self.napcatIssuesButton, 0, Qt.AlignmentFlag.AlignLeft)
        self.viewLayout.addWidget(self.NCDIssuesButton, 0, Qt.AlignmentFlag.AlignLeft)

        # 设置对话框
        self.widget.setMinimumWidth(300)
        self.cancelButton.hide()
