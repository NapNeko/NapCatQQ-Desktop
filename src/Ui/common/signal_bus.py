# -*- coding: utf-8 -*-

"""
NCD 信号总线
"""
from PySide6.QtCore import Signal, QObject


class SettingsSignalBus(QObject):
    """设置信号总线"""

    # 设置页面相关信号

    # 个性化设置相关信号
    commandCenterSingal = Signal(bool)  # 是否隐藏命令中心


settingsSignalBus = SettingsSignalBus()


__all__ = ["settingsSignalBus"]
