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

    log_buffer: list[Log]

    log_buffer_size: int
    log_buffer_delete_size: int

    def __init__(self):
        """初始化日志记录器"""

        # Log 缓冲区
        self.log_buffer = []

        # 配置项
        self.log_buffer_size = 5000  # 日志缓冲区大小
        self.log_buffer_delete_size = 1000  # 删除缓冲区日志数量
        self.log_save_day = 7  # 日志保存天数

    def createLogFile(self):
        """
        ## 用于创建日志文件
            - 日志文件名格式为: {DATETIME}.log
        """
        # 定义日志文件路径
        self.log_path = PathFunc().log_path / f"{datetime.now().strftime('%Y-%m-%d %H-%M-%S')}.log"

        # 遍历日志文件夹, 删除过期日志文件(超过 7 天)
        for log_file in PathFunc().log_path.iterdir():
            if (datetime.now() - datetime.fromtimestamp(log_file.stat().st_mtime)).days > self.log_save_day:
                log_file.unlink()

    @staticmethod
    def toStringLog(log_buffer: list[Log], file_path: Path | str):
        """
        ## 持久化日志
            - 将日志转为字符串保存到文件中
        """
        # 遍历日志列表, 追加到日志文件中
        with open(file_path, "a", encoding="utf-8") as f:
            [f.write(log.toString() + "\n") for log in log_buffer]

    # 清理缓冲区
    def clearBuffer(self):
        """
        ## 清理日志缓冲区
            - 当日志列表长度超过 日志缓冲区大小 时, 进行持久化, 并根据 删除缓冲区日志数量 清理缓冲区
        """
        if len(self.log_buffer) >= self.log_buffer_size:
            self.toStringLog(self.log_buffer, self.log_path)
            self.log_buffer = self.log_buffer[self.log_buffer_delete_size :]

    def _log(
        self,
        level: LogLevel,
        message: str,
        time: int | float,
        log_type: LogType,
        log_source: LogSource,
        log_position: LogPosition,
    ):
        """
        ## 构造 Log 对象

        ## 参数
            - level: LogLevel - 日志等级
            - message: str - 信息内容
            - time: int | float - 时间戳
            - log_type: LogType - 日志类型
            - log_source: LogSource - 日志来源
            - log_position: LogPosition - 日志位置
        """
        # 构造 Log 并添加到列表
        self.log_buffer.append(Log(level, message, time, log_type, log_source, log_position))
        # 判断是否要进行持久化
        self.clearBuffer()
        # 打印 log
        print(self.log_buffer[-1])

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
        self._log(LogLevel.DBUG, message, datetime.now().timestamp(), log_type, log_source, log_position)

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
        self._log(LogLevel.INFO, message, datetime.now().timestamp(), log_type, log_source, log_position)

    @capture_call_location
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
        self._log(LogLevel.WARN, message, datetime.now().timestamp(), log_type, log_source, log_position)

    @capture_call_location
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
        self._log(LogLevel.EROR, message, datetime.now().timestamp(), log_type, log_source, log_position)


# 实例化日志记录器
logger = Logger()
logger.createLogFile()


if __name__ == "__main__":
    for i in range(2000):
        logger.info("info")
        logger.error("error")
        logger.warning("warning")

    print(logger.log_buffer)
    print(len(logger.log_buffer))
