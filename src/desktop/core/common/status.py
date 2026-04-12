# -*- coding: utf-8 -*-
# 标准库导入
from enum import Enum


class ButtonStatus(Enum):
    """按钮显示状态"""

    INSTALL = 0
    UNINSTALLED = 1
    UPDATE = 2
    NONE = 3


class ProgressRingStatus(Enum):
    """进度环显示状态"""

    INDETERMINATE = 0
    DETERMINATE = 1
    NONE = 2


class StatusLabel(Enum):
    """状态标签显示状态"""

    SHOW = 0
    HIDE = 1
