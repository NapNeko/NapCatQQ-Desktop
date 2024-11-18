# -*- coding: utf-8 -*-
# 标准库导入
from pathlib import Path
from datetime import datetime

# 项目内模块导入
from src.Core.Utils.logger import LogLevel
from src.Core.Utils.PathFunc import PathFunc
from src.Core.Utils.logger.log_data import Log, LogPosition
from src.Core.Utils.logger.log_enum import LogType, LogSource
from src.Core.Utils.logger.log_utils import capture_call_location


class Logger:
    """NCD 内部日志记录器"""

    log_list: list[Log]
    debug_list: list[Log]
    info_list: list[Log]
    warning_list: list[Log]
    error_list: list[Log]
    critical_list: list[Log]

    def __init__(self):
        """初始化日志记录器"""

        # 总 Log 日志以及按等级分类的 Log
        self.log_list = []
        self.debug_list = []
        self.info_list = []
        self.warning_list = []
        self.error_list = []
        self.critical_list = []

    def createLogFile(self):
        """
        ## 用于创建日志文件
            - 日志文件名格式为: {LEVEL}.{DATETIME}.log
        """
        # 定义日志文件路径
        self.info_path = PathFunc().log_info_path / f"INFO.{datetime.now().strftime('%Y-%m-%d %H-%M-%S')}.log"
        self.debug_path = PathFunc().log_debug_path / f"DEBUG.{datetime.now().strftime('%Y-%m-%d %H-%M-%S')}.log"

        # 遍历日志文件夹, 删除过期日志文件(超过 7 天)
        for log_file in PathFunc().log_info_path.iterdir():
            if (datetime.now() - datetime.fromtimestamp(log_file.stat().st_mtime)).days > 7:
                log_file.unlink()

        for log_file in PathFunc().log_debug_path.iterdir():
            if (datetime.now() - datetime.fromtimestamp(log_file.stat().st_mtime)).days > 7:
                log_file.unlink()

        # 创建日志文件, 777 权限, 文件存在则覆盖
        self.info_path.touch(mode=0o777, exist_ok=True)
        self.debug_path.touch(mode=0o777, exist_ok=True)

    @staticmethod
    def toStringLog(log_list: list[Log], file_path: Path | str):
        """
        ## 持久化日志
            - 将日志转为字符串保存到文件中
        """
        # 遍历日志列表, 追加到日志文件中
        with open(file_path, "a", encoding="utf-8") as f:
            [f.write(log.toString() + "\n") for log in log_list]

    @capture_call_location
    def debug(
        self,
        message: str,
        log_type: LogType = LogType.NONE_TYPE,
        log_source: LogSource = LogSource.NONE,
        log_position: LogPosition = None,
    ):
        """
        ## debug 消息记录

        ## 参数
            - message: str - 信息内容
            - log_type: LogType - 日志类型
            - log_source: LogSource - 日志来源
            - log_position: LogPosition - 日志位置
        """
        # 构建 Log 对象
        log = Log(LogLevel.DBUG, message, datetime.now().timestamp(), log_type, log_source, log_position)

        # 添加到日志列表
        self.log_list.append(log)
        self.debug_list.append(log)

        # 判断是否要进行持久化
        if len(self.debug_list) >= 2000:
            self.toStringLog(self.debug_list, self.debug_path)
            self.debug_list = []

        # 判断是否要清理缓冲区, 当日志列表长度超过 2000 时, 删除前 1000 条日志
        if len(self.log_list) >= 2000:
            self.log_list = self.log_list[1000:]

        # 打印 log
        print(log)

    @capture_call_location
    def info(
        self,
        message: str,
        log_type: LogType = LogType.NONE_TYPE,
        log_source: LogSource = LogSource.NONE,
        log_position: LogPosition = None,
    ):
        """
        ## info 消息记录

        ## 参数
            - message: str - 信息内容
            - log_type: LogType - 日志类型
            - log_source: LogSource - 日志来源
            - log_position: LogPosition - 日志位置
        """
        # 构建 Log 对象
        log = Log(LogLevel.INFO, message, datetime.now().timestamp(), log_type, log_source, log_position)

        # 添加到日志列表
        self.log_list.append(log)
        self.info_list.append(log)

        # 判断是否要进行持久化
        if len(self.info_list) >= 2000:
            self.toStringLog(self.info_list, self.info_path)
            self.info_list = []

        # 判断是否要清理缓冲区, 当日志列表长度超过 2000 时, 删除前 1000 条日志
        if len(self.log_list) >= 2000:
            self.log_list = self.log_list[1000:]

        # 打印 log
        print(log)

    def warning(
        self,
        message: str,
        log_type: LogType = LogType.NONE_TYPE,
        log_source: LogSource = LogSource.NONE,
        log_position: LogPosition = None,
    ):
        """
        ## warning 消息记录

        ## 参数
            - message: str - 信息内容
            - log_type: LogType - 日志类型
            - log_source: LogSource - 日志来源
            - log_position: LogPosition - 日志位置
        """
        # 构建 Log 对象
        log = Log(LogLevel.WARN, message, datetime.now().timestamp(), log_type, log_source, log_position)

        # 添加到日志列表
        self.log_list.append(log)
        self.warning_list.append(log)

        # 判断是否要清理缓冲区, 当日志列表长度超过 2000 时, 删除前 1000 条日志
        if len(self.log_list) >= 2000:
            self.log_list = self.log_list[1000:]
        if len(self.warning_list) >= 2000:
            self.warning_list = self.warning_list[1000:]

        # 打印 log
        print(log)

    def error(
        self,
        message: str,
        log_type: LogType = LogType.NONE_TYPE,
        log_source: LogSource = LogSource.NONE,
        log_position: LogPosition = None,
    ):
        """
        ## error 消息记录

        ## 参数
            - message: str - 信息内容
            - log_type: LogType - 日志类型
            - log_source: LogSource - 日志来源
            - log_position: LogPosition - 日志位置
        """
        # 构建 Log 对象
        log = Log(LogLevel.EROR, message, datetime.now().timestamp(), log_type, log_source, log_position)

        # 添加到日志列表
        self.log_list.append(log)
        self.error_list.append(log)

        # 判断是否要清理缓冲区, 当日志列表长度超过 2000 时, 删除前 1000 条日志
        if len(self.log_list) >= 2000:
            self.log_list = self.log_list[1000:]
        if len(self.error_list) >= 2000:
            self.error_list = self.error_list[1000:]

        # 打印 log
        print(log)

    def critical(
        self,
        message: str,
        log_type: LogType = LogType.NONE_TYPE,
        log_source: LogSource = LogSource.NONE,
        log_position: LogPosition = None,
    ):
        """
        ## critical 消息记录

        ## 参数
            - message: str - 信息内容
            - log_type: LogType - 日志类型
            - log_source: LogSource - 日志来源
            - log_position: LogPosition - 日志位置
        """
        # 构建 Log 对象
        log = Log(LogLevel.CRIT, message, datetime.now().timestamp(), log_type, log_source, log_position)

        # 添加到日志列表
        self.log_list.append(log)
        self.critical_list.append(log)

        # 判断是否要清理缓冲区, 当日志列表长度超过 2000 时, 删除前 1000 条日志
        if len(self.log_list) >= 2000:
            self.log_list = self.log_list[1000:]
        if len(self.critical_list) >= 2000:
            self.critical_list = self.critical_list[1000:]

        # 打印 log
        print(log)


# 实例化日志记录器
logger = Logger()
logger.createLogFile()
