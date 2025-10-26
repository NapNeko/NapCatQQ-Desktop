# -*- coding: utf-8 -*-
"""这是 Bot 列表子页面模块"""

# 第三方库导入
from qfluentwidgets import FlowLayout, ScrollArea
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.core.config.config_model import Config
from src.core.config.operate_config import read_config

from ..widget.card import BotCard


class BotListPage(ScrollArea):
    """Bot 列表子页面"""

    def __init__(self, parent: QWidget | None = None) -> None:
        """构造函数"""
        super().__init__(parent)
        # 设置属性
        self._bot_config_list: list[Config] = []
        self._bot_card_list: list[BotCard] = []

        # 创建视图和布局
        self.view = QWidget(self)
        self.view_layout = FlowLayout(self.view)

        # 设置控件
        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.view_layout.setContentsMargins(0, 0, 0, 0)
        self.view_layout.setSpacing(4)

        # 调用方法
        self.update_bot_list()

    # ==================== 公共方法 ====================
    def update_bot_list(self) -> None:
        """刷新 Bot 列表

        用于刷新 view 中的 Bot Card
        """
        # 判断原有 bot config list 是否为空, 不为空则清空
        if self._bot_card_list:
            self.remove_all_bot()

        # 读取配置文件
        if (configs := read_config()) == self._bot_config_list:
            # 如果读取的配置文件与现有配置文件一致, 则跳过
            return
        else:
            # 不一致则赋值给属性
            self._bot_config_list = configs.copy()

        # 创建 Bot Card 并添加到布局
        for config in self._bot_config_list:
            card = BotCard(config)
            card.remove_signal.connect(self.remove_bot_by_qqid)
            self._bot_card_list.append(card)
            self.view_layout.addWidget(card)

    def remove_bot_by_qqid(self, qqid: str) -> None:
        """通过 QQID 移除 Bot Card

        用于移除 view 中指定 QQID 的 Bot Card
        """
        for card in self._bot_card_list:
            if str(card._config.bot.QQID) == qqid:
                self._bot_card_list.remove(card)
                self.view_layout.removeWidget(card)
                card.setParent(None)
                card.deleteLater()
                break

    def remove_all_bot(self) -> None:
        """移除所有 Bot Card

        用于移除 view 中的所有 Bot Card
        """
        self._bot_config_list.clear()
        self.view_layout.takeAllWidgets()
