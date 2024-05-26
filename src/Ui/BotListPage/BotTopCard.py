# -*- coding: utf-8 -*-
from typing import TYPE_CHECKING

from PySide6.QtCore import Qt
from PySide6.QtGui import QFont
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget
from creart import it
from qfluentwidgets import CaptionLabel, InfoBar, InfoBarPosition, ToolTipFilter
from qfluentwidgets.common import setFont, FluentIcon
from qfluentwidgets.components import BreadcrumbBar, TransparentToolButton

if TYPE_CHECKING:
    from src.Ui.BotListPage.BotListWidget import BotListWidget


class BotTopCard(QWidget):
    """
    ## BotListWidget 顶部展示的 Card

    用于展示 Breadcrumb navigation 以及一些操作按钮
    """

    def __init__(self, parent: "BotListWidget"):
        super().__init__(parent=parent)

        # 创建所需控件
        self.breadcrumbBar = BreadcrumbBar(self)
        self.subtitleLabel = CaptionLabel(self.tr("All the bots you've added are here"))
        self.updateListButton = TransparentToolButton(FluentIcon.SYNC)  # 刷新列表按钮

        self.hBoxLayout = QHBoxLayout()
        self.labelLayout = QVBoxLayout()
        self.buttonLayout = QHBoxLayout()

        # 设置控件
        setFont(self.breadcrumbBar, 28, QFont.Weight.DemiBold)
        self.breadcrumbBar.addItem(routeKey="BotTopCardTitle", text=self.tr("Bot List"))
        self.breadcrumbBar.setSpacing(15)
        self.updateListButton.clicked.connect(self._updateListButtonSolt)
        self.breadcrumbBar.currentIndexChanged.connect(self._breadcrumbBarSolt)

        self._addTooltips()
        self._setLayout()

    def addItem(self, routeKey: str):
        """
        ## 给 breadcrumbBar 添加 item 项
        """
        self.breadcrumbBar.addItem(routeKey, routeKey)

    def _addTooltips(self):
        """
        ## 为按钮添加悬停提示
        """
        # 添加提示
        self.updateListButton.setToolTip(self.tr("Click to refresh the list"))
        self.updateListButton.installEventFilter(ToolTipFilter(self.updateListButton))

    def _breadcrumbBarSolt(self, index: int):
        """
        ## 判断用户是否点击的是 Bot List
        如果是则返回 Bot List 页面
        """
        if index == 0:
            from src.Ui.BotListPage.BotListWidget import BotListWidget
            it(BotListWidget).view.setCurrentIndex(index)
            self.updateListButton.show()

    @staticmethod
    def _updateListButtonSolt():
        """
        ## 更新列表按钮的槽函数
        """
        from src.Ui.BotListPage.BotListWidget import BotListWidget
        it(BotListWidget).botList.updateList()

    def _setLayout(self):
        """
        ## 对内部进行布局
        """
        self.labelLayout.setSpacing(0)
        self.labelLayout.setContentsMargins(0, 0, 0, 0)
        self.labelLayout.addWidget(self.breadcrumbBar)
        self.labelLayout.addSpacing(5)
        self.labelLayout.addWidget(self.subtitleLabel)

        self.buttonLayout.setSpacing(0)
        self.buttonLayout.setContentsMargins(0, 0, 0, 0)
        self.buttonLayout.addWidget(self.updateListButton)
        self.buttonLayout.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignBottom)

        self.hBoxLayout.addLayout(self.labelLayout)
        self.hBoxLayout.addLayout(self.buttonLayout)
        self.hBoxLayout.setContentsMargins(0, 0, 20, 0)

        self.setLayout(self.hBoxLayout)
