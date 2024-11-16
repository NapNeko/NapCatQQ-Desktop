# -*- coding: utf-8 -*-
# 标准库导入
from abc import ABC
from typing import TYPE_CHECKING, Self, Optional

# 第三方库导入
from creart import add_creator, exists_module
from creart.creator import AbstractCreator, CreateTargetInfo
from PySide6.QtWidgets import QWidget, QVBoxLayout, QStackedWidget

# 项目内模块导入
from src.Core.Config import cfg
from src.Ui.StyleSheet import StyleSheet
from src.Ui.common.widget import BackgroundWidget
from src.Core.Utils.singleton import singleton
from src.Ui.BotListPage.BotList import BotList
from src.Ui.common.stacked_widget import TransparentStackedWidget
from src.Ui.BotListPage.BotTopCard import BotTopCard

if TYPE_CHECKING:
    # 项目内模块导入
    from src.Ui.MainWindow import MainWindow


@singleton
class BotListWidget(BackgroundWidget):
    view: Optional[TransparentStackedWidget]
    topCard: Optional[BotTopCard]
    botList: Optional[BotList]
    vBoxLayout: Optional[QVBoxLayout]

    def __init__(self) -> None:
        super().__init__()

        # 传入配置
        self.bgEnabledConfig = cfg.bgListPage
        self.bgPixmapLightConfig = cfg.bgListPageLight
        self.bgPixmapDarkConfig = cfg.bgListPageDark
        self.bgOpacityConfig = cfg.bgListPageOpacity

        # 调用方法
        super().updateBgImage()

    def initialize(self, parent: "MainWindow") -> Self:
        """
        初始化
        """
        self.vBoxLayout = QVBoxLayout(self)

        self.topCard = BotTopCard(self)
        self.view = TransparentStackedWidget(self)
        self.botList = BotList(self.view)

        # 设置 QWidget
        self.setParent(parent),
        self.setObjectName("BotListPage")
        self.view.setObjectName("BotListStackedWidget")
        self.view.addWidget(self.botList)
        self.view.setCurrentWidget(self.botList)

        # 调用方法
        self._setLayout()

        # 应用样式表
        StyleSheet.BOT_LIST_WIDGET.apply(self)

        return self

    def _setLayout(self) -> None:
        """
        ## 对内部进行布局
        """
        self.vBoxLayout.addWidget(self.topCard)
        self.vBoxLayout.addWidget(self.view)
        self.vBoxLayout.setContentsMargins(24, 20, 24, 10)
        self.setLayout(self.vBoxLayout)

    def runAllBot(self) -> None:
        """
        ## 运行所有机器人
        """
        # 项目内模块导入
        from src.Ui.BotListPage import BotListWidget
        from src.Ui.BotListPage.BotWidget import BotWidget

        for card in self.botList.botCardList:

            if card.botWidget is None:
                card.botWidget = BotWidget(card.config)
                self.view.addWidget(card.botWidget)

            card.botWidget.runButtonSlot()

    def stopAllBot(self):
        """
        ## 停止所有 card
        """
        for card in self.botList.botCardList:
            if not card.botWidget:
                continue
            if card.botWidget.isRun:
                card.botWidget.stopButton.click()

    def getBotIsRun(self):
        """
        ## 获取是否有 card 正在运行
        """
        for card in self.botList.botCardList:
            if not card.botWidget:
                # 如果没有创建则表示没有运行
                continue
            if card.botWidget.isRun:
                return True
