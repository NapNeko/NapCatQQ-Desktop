# -*- coding: utf-8 -*-
# 标准库导入
from typing import List

# 第三方库导入
from qfluentwidgets import FlowLayout, ScrollArea
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.core.config.operate_config import read_config
from src.ui.components.info_bar import success_bar
from src.ui.page.bot_list_page.bot_card import BotCard


class BotList(ScrollArea):
    """机器人列表滚动区域，用于显示和管理所有已配置的机器人卡片"""

    def __init__(self, parent: QWidget) -> None:
        """初始化机器人列表

        Args:
            parent: 父控件
        """
        super().__init__(parent=parent)
        self.bot_card_list: List[BotCard] = []

        self._create_view()
        self._setup_ui()
        self.update_list()

    def _setup_ui(self) -> None:
        """设置滚动区域的UI属性"""
        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

    def _create_view(self) -> None:
        """创建并配置滚动区域的内容视图"""
        self.view = QWidget(self)
        self.view.setObjectName("BotListView")

        self.cardLayout = FlowLayout(self.view, True)
        self.cardLayout.setContentsMargins(0, 0, 0, 0)
        self.cardLayout.setSpacing(4)

        self.view.setLayout(self.cardLayout)

    def update_list(self) -> None:
        """更新机器人列表，根据配置文件同步添加或删除机器人卡片"""
        # 读取配置文件，如果没有配置且当前没有卡片则直接返回
        bot_configs = read_config()
        if not bot_configs and not self.bot_card_list:
            return

        if not self.bot_card_list:
            # 首次运行：直接添加所有配置项到布局和卡片列表
            for config in bot_configs:
                card = BotCard(config, self)
                self.cardLayout.addWidget(card)
                self.bot_card_list.append(card)
            success_bar(self.tr(f"成功从配置文件加载 {len(bot_configs)} 个配置项"))
            return

        # 非首次运行：检查新增和删除的配置
        qq_id_list = [card.config.bot.qq_id for card in self.bot_card_list]

        # 检查新增的机器人配置
        for bot_config in bot_configs:
            if bot_config.bot.qq_id not in qq_id_list:
                new_card = BotCard(bot_config)
                self.cardLayout.addWidget(new_card)
                self.bot_card_list.append(new_card)

        # 检查已删除的机器人配置
        for card in self.bot_card_list[:]:  # 使用切片创建副本避免迭代时修改
            if card.config not in bot_configs:
                self.bot_card_list.remove(card)
                self.cardLayout.removeWidget(card)
                card.deleteLater()

        success_bar(self.tr("更新机器人列表成功!"))

        # 刷新布局
        self.cardLayout.update()
