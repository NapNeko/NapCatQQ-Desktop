# -*- coding: utf-8 -*-
from typing import TYPE_CHECKING

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout
from creart import it
from qfluentwidgets import SegmentedWidget, TransparentToolButton, FluentIcon, ToolTipFilter

from src.Core.Config.ConfigModel import Config


class BotWidget(QWidget):
    """
    ## 机器人卡片对应的 Widget
    """

    def __init__(self, config: Config):
        super().__init__()
        self.config = config
        # 创建所需控件
        self._createPivot()
        self._createButton()

        self.vBoxLayout = QVBoxLayout()
        self.hBoxLayout = QHBoxLayout()
        self.buttonLayout = QHBoxLayout()

        # 调用方法
        self._setLayout()
        self._addTooltips()

    def _createPivot(self) -> None:
        """
        ## 创建机器人 Widget 顶部导航栏
        """
        self.pivot = SegmentedWidget(self)
        self.pivot.addItem(
            routeKey=f"{self.config.bot.QQID}_BotWidgetPivot_BotInfo",
            text=self.tr("Bot info")
        )
        self.pivot.addItem(
            routeKey=f"{self.config.bot.QQID}_BotWidgetPivot_BotSetup",
            text=self.tr("Bot Setup")
        )
        self.pivot.addItem(
            routeKey=f"{self.config.bot.QQID}_BotWidgetPivot_BotLog",
            text=self.tr("Bot Log")
        )
        self.pivot.setCurrentItem(f"{self.config.bot.QQID}_BotWidgetPivot_BotInfo")
        self.pivot.setMaximumWidth(300)

    def _createButton(self):
        """
        ## 创建按钮
        """
        self.returnListButton = TransparentToolButton(FluentIcon.RETURN)  # 返回到列表按钮
        self.returnListButton.clicked.connect(self._returnListButtonSolt)

    def _addTooltips(self):
        """
        ## 为按钮添加悬停提示
        """
        self.returnListButton.setToolTip(self.tr("Click Back to list"))
        self.returnListButton.installEventFilter(ToolTipFilter(self.returnListButton))

    @staticmethod
    def _returnListButtonSolt():
        """
        ## 返回列表按钮的槽函数
        """
        from src.Ui.BotListPage.BotListWidget import BotListWidget
        it(BotListWidget).view.setCurrentIndex(0)
        it(BotListWidget).topCard.breadcrumbBar.setCurrentIndex(0)
        it(BotListWidget).topCard.updateListButton.show()

    def _setLayout(self) -> None:
        """
        对内部进行布局
        """
        self.buttonLayout.setSpacing(0)
        self.buttonLayout.setContentsMargins(0, 0, 0, 0)
        self.buttonLayout.addWidget(self.returnListButton)
        self.buttonLayout.setAlignment(Qt.AlignmentFlag.AlignRight)

        self.hBoxLayout.setSpacing(0)
        self.hBoxLayout.setContentsMargins(0, 0, 0, 0)
        self.hBoxLayout.addWidget(self.pivot)
        self.hBoxLayout.addLayout(self.buttonLayout)

        self.vBoxLayout.setSpacing(0)
        self.vBoxLayout.setContentsMargins(0, 0, 0, 0)
        self.vBoxLayout.addLayout(self.hBoxLayout)
        self.vBoxLayout.addStretch(1)

        self.setLayout(self.vBoxLayout)
