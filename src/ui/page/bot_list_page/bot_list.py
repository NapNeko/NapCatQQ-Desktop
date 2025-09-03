# -*- coding: utf-8 -*-
# 标准库导入
from typing import List

# 第三方库导入
from qfluentwidgets import FlowLayout, ScrollArea
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.ui.components.info_bar import success_bar
from src.ui.page.bot_list_page.bot_card import BotCard
from src.core.config.operate_config import read_config


class BotList(ScrollArea):
    """
    ## BotListWidget 内部的机器人列表
    """

    def __init__(self, parent) -> None:
        """
        ## 初始化
        """
        super().__init__(parent=parent)
        # 创建属性
        self.botCardList: List[BotCard] = []

        # 调用方法
        self._createView()
        self._initWidget()
        self.updateList()

    def _initWidget(self) -> None:
        """
        ## 设置 ScrollArea
        """
        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

    def _createView(self) -> None:
        """
        ## 构建并设置 ScrollArea 所需的 widget
        """
        self.view = QWidget(self)
        self.cardLayout = FlowLayout(self.view, True)
        self.cardLayout.setContentsMargins(0, 0, 0, 0)
        self.cardLayout.setSpacing(4)
        self.view.setObjectName("BotListView")
        self.view.setLayout(self.cardLayout)

    def updateList(self) -> None:
        """
        ## 更新机器人列表
        """

        if not (bot_configs := read_config()) and not self.botCardList:
            return

        if not self.botCardList:
            # 如果是首次运行, 则直接添加到布局和 botCardList
            for config in bot_configs:
                card = BotCard(config, self)
                self.cardLayout.addWidget(card)
                self.botCardList.append(card)
            success_bar(self.tr(f"成功从配置文件加载 {len(bot_configs)} 个配置项"))
            return

        qq_id_list = [card.config.bot.QQID for card in self.botCardList]  # 取出卡片列表中卡片的 QQID
        for bot_config in bot_configs:
            # 遍历并判断是否有新增的 bot 配置
            if bot_config.bot.QQID not in qq_id_list:
                # 不属于则就属于新增, 创建 card 并添加到布局
                new_card = BotCard(bot_config)
                self.cardLayout.addWidget(new_card)
                self.botCardList.append(new_card)

        for card in self.botCardList:
            # 遍历并判断是否有减少的 bot 配置
            if card.config not in bot_configs:
                # 不属于则代表已经删除, 移除出布局并删除
                self.botCardList.remove(card)
                self.cardLayout.removeWidget(card)
                card.deleteLater()

        success_bar(self.tr("更新机器人列表成功!"))

        # 刷新一次布局
        self.cardLayout.update()
