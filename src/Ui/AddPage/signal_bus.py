# -*- coding: utf-8 -*-

# 标准库导入
from enum import Enum

from PySide6.QtCore import Signal, QObject


class AddPageSignalBus(QObject):
    """添加页面信号总线"""

    addWidgetViewChange = Signal(int)  # 添加页面视图切换信号, 带切换的索引
    addConnectConfigButtonClicked = Signal()  # 添加网络配置按钮点击信号
    chooseConnectType = Signal(Enum)  # 选择连接类型信号, 带连接类型的字符串


addPageSingalBus = AddPageSignalBus()


__all__ = ["addPageSingalBus"]
