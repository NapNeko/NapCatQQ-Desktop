# -*- coding: utf-8 -*-
# 标准库导入
from enum import Enum


class ButtonStatus(Enum):
    """
    ## 状态枚举类
    """

    # 安装状态
    INSTALL = 0
    UNINSTALLED = 1

    # 更新状态
    UPDATE = 2

    # 隐藏所有
    NONE = 3


class ProgressRingStatus(Enum):
    """
    ## 进度环状态
    """

    # 显示确定进度环和不确定进度环
    INDETERMINATE = 0
    DETERMINATE = 1

    # 隐藏全部进度环
    NONE = 2


class StatusLabel(Enum):
    """
    ## 状态标签
    """

    # 显示状态标签
    SHOW = 0
    HIDE = 1
