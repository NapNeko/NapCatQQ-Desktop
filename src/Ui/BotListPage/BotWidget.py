# -*- coding: utf-8 -*-
from typing import TYPE_CHECKING
from PySide6.QtWidgets import QWidget
from qfluentwidgets import Pivot

from src.Core.Config.ConfigModel import Config


class BotWidget(QWidget):
    """
    ## 机器人卡片对应的 Widget
    """

    def __init__(self, config: Config):
        super().__init__()
        self.config = config

        self._createPivot()

    def _createPivot(self):
        """
        ## 创建机器人 Widget 顶部导航栏
        """
        self.pivot = Pivot(self)
        self.pivot.addItem(
            routeKey=f"{self.config.bot.QQID}_BotWidgetPivot_BotInfo",
            text=self.tr("Bot info")
        )
        self.pivot.addItem(
            routeKey=f"{self.config.bot.QQID}_BotWidgetPivot_BotLog",
            text=self.tr("Bot Log")
        )


