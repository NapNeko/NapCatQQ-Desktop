# -*- coding: utf-8 -*-
"""
# 日志类型格式化输出需要统一宽度
    - LogLevel: 8
    - LogType: 8
    - LogSource: 6
"""

# 标准库导入
from enum import Enum


class LogLevel(Enum):
    """日志等级"""

    DBUG = 0  # debug
    INFO = 1  # information
    WARN = 2  # warning
    EROR = 3  # error
    CRIT = 4  # critical
    ALL_ = 5  # all

    def __str__(self):
        return f"[{self.name}]"


class LogType(Enum):
    """日志类型"""

    FILE_FUNC = 0  # 文件操作
    NETWORK = 1  # 网络操作
    NONE_TYPE = 2  # 无类型

    def __str__(self):
        # 格式化输出，固定宽度为 11
        return f"[{self.name.center(11)}]"


class LogSource(Enum):
    """日志来源"""

    UI = 0  # 用户界面
    CORE = 1  # 核心逻辑
    NONE = 2  # 无来源

    def __str__(self):
        # 格式化输出，固定宽度为 6
        return f"[{self.name.center(6)}]"
