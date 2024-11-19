# -*- coding: utf-8 -*-
# 标准库导入
from datetime import datetime
from dataclasses import field, dataclass

# 项目内模块导入
from src.Core.Utils.logger.log_enum import LogType, LogLevel, LogSource


@dataclass(frozen=True)
class LogPosition:
    """日志模块位置, 用于定位日志来源"""

    module: str
    file: str
    line: int

    def __str__(self):
        return f"[{self.module} > {self.file}:{self.line}]"


@dataclass(frozen=True)
class Log:
    """ 基本日志内容 """

    # 基本信息
    level: LogLevel  # 日志等级
    message: str  # 日志消息
    time: int | float  # 日志时间

    # 详细信息
    log_type: LogType  # 日志类型
    source: LogSource  # 日志来源
    position: LogPosition  # 日志位置

    def __str__(self):
        time = datetime.fromtimestamp(self.time).strftime("%y-%m-%d %H:%M:%S")
        return f"{time} | {self.level} | {self.message}"

    def toString(self):
        """
        ## 转为字符串
        """
        time = datetime.fromtimestamp(self.time).strftime("%y-%m-%d %H:%M:%S")
        return f"{time} | {self.level} | {self.log_type} | {self.source} | {self.position} | {self.message}"


@dataclass()
class LogChunk:
    """ 日志块 """
    log_title: Log
    log_end: Log
    logs: list[Log] = field(default_factory=list)
