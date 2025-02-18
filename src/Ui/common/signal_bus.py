# -*- coding: utf-8 -*-

"""
NCD 信号总线
"""
from PySide6.QtCore import Signal, QObject


class SettingsSignalBus(QObject):
    """设置信号总线"""

    # 信号
    commandCenterSingal = Signal(bool)  # 是否隐藏命令中心


settingsSignalBus = SettingsSignalBus()
