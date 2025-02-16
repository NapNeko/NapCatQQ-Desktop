# -*- coding: utf-8 -*-

"""
NCD 信号总线
"""
from PySide6.QtCore import Signal, QObject


class SignalBus(QObject):
    """信号总线"""

    # 不知道后续要不要做扩展


signalBus = SignalBus()
