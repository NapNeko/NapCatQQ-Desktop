# -*- coding: utf-8 -*-
from typing import TYPE_CHECKING

from PySide6.QtGui import QFont
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget
from qfluentwidgets import CaptionLabel
from qfluentwidgets.common import setFont
from qfluentwidgets.components import (
    BreadcrumbBar,
)

if TYPE_CHECKING:
    from src.Ui.BotListPage.BotList import BotListWidget


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
        self.hBoxLayout = QHBoxLayout()
        self.labelLayout = QVBoxLayout()

        # 设置控件
        setFont(self.breadcrumbBar, 28, QFont.Weight.DemiBold)
        self.breadcrumbBar.addItem(routeKey="BotTopCardTitle", text=self.tr("Bot List"))
        self.breadcrumbBar.setSpacing(15)

        self._setLayout()

    def _setLayout(self):
        """
        ## 对内部进行布局
        """
        self.labelLayout.setSpacing(0)
        self.labelLayout.setContentsMargins(0, 0, 0, 0)
        self.labelLayout.addWidget(self.breadcrumbBar)
        self.labelLayout.addSpacing(5)
        self.labelLayout.addWidget(self.subtitleLabel)

        self.hBoxLayout.addLayout(self.labelLayout)

        self.setLayout(self.hBoxLayout)
