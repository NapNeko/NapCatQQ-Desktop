# -*- coding: utf-8 -*-

# 标准库导入
from enum import Enum

from PySide6.QtCore import QObject, Signal


class BotListPageSignalBus(QObject):
    """添加页面信号总线"""

    add_widget_view_change_signal = Signal(int)  # 添加页面视图切换信号, 带切换的索引
    add_connect_config_button_signal = Signal()  # 添加网络配置按钮点击信号
    choose_connect_type_signal = Signal(Enum)  # 选择连接类型信号, 带连接类型的字符串
    remove_card_signal = Signal(QObject)  # 删除卡片信号, 带卡片


bot_list_page_signal_bus = BotListPageSignalBus()
