# -*- coding: utf-8 -*-
# 标准库导入
from pathlib import Path
from datetime import datetime

# 项目内模块导入
from src.Core.Utils.logger.log_data import Log, LogPosition
from src.Core.Utils.logger.log_enum import LogType, LogLevel, LogSource
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

    def load_config(self):
        """
        ## 加载配置项
        """
        self.log_buffer_size = 5000  # 日志缓冲区大小
        self.log_buffer_delete_size = 1000  # 删除缓冲区日志数量
        self.log_save_day = 7  # 日志保存天数

    def createLogFile(self):
        """
        ## 用于创建日志文件
            - 日志文件名格式为: {DATETIME}.log
        """

        # 定义日志文件路径
        if not (log_dir := Path.cwd() / "log").exists():
            log_dir.mkdir(parents=True, exist_ok=True)
        self.log_path = log_dir / f"{datetime.now().strftime('%Y-%m-%d_%H-%M-%S')}.log"

        # 遍历日志文件夹, 删除过期日志文件(超过 7 天)
        for log_file in log_dir.iterdir():
            if (datetime.now() - datetime.fromtimestamp(log_file.stat().st_mtime)).days > self.log_save_day:
                log_file.unlink()

    def clearBuffer(self):
        """
        ## 清理日志缓冲区
            - 当日志列表长度超过 日志缓冲区大小 时, 并根据 删除缓冲区日志数量 清理缓冲区
        """
        if len(self.log_buffer) >= self.log_buffer_size:
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
        log = Log(level, message, time, log_type, log_source, log_position)
        self.log_buffer.append(log)
        # 遍历日志列表, 追加到日志文件中
        with open(self.log_path, "a", encoding="utf-8") as f:
            f.write(log.toString() + "\n")
        # 判断是否需要清理缓冲区
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


class LoggerChunk(Logger):
    """日志块功能实现"""

    logger: Logger

    def __init__(self):
        """初始化日志块"""
        super().__init__()

        # 日志记录器
        self.logger = logger

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
        log = Log(level, message, time, log_type, log_source, log_position)
        self.log_buffer.append(log)
        # 遍历日志列表, 追加到日志文件中
        with open(logger.log_path, "a", encoding="utf-8") as f:
            f.write(log.toString() + "\n")
        # 打印日志
        print(self.log_buffer[-1])

    @capture_call_location
    def start(self, text: str, log_type: LogType, log_source: LogSource, log_position: LogPosition = None):
        """
        ## 日志块标头

        ## 参数
            - text: str - 日志块名称
            - log_type: LogType - 日志类型
            - log_source: LogSource - 日志来源
            - log_position: LogPosition - 日志位置
        """
        text = f"{'-'*20} > {text} < {'-'*20}"
        self._log(LogLevel.INFO, text, datetime.now().timestamp(), log_type, log_source, log_position)

    @capture_call_location
    def end(self, text: str, log_type: LogType, log_source: LogSource, log_position: LogPosition = None):
        """
        ## 日志块结尾

        ## 参数
            - text: str - 日志块名称
            - log_type: LogType - 日志类型
            - log_source: LogSource - 日志来源
            - log_position: LogPosition - 日志位置
        """
        text = f"{'-'*20} > {text} < {'-'*20}"
        self._log(LogLevel.INFO, text, datetime.now().timestamp(), log_type, log_source, log_position)
        self.logger.log_buffer.append(self)

    def __str__(self):
        """输出日志块开头,内容结尾"""
        return "\n".join([log.toString() for log in self.log_chunk])

    def toString(self):
        """
        ## 转为字符串
        """
        return "\n".join(
            f"{datetime.fromtimestamp(log.time).strftime('%y-%m-%d %H:%M:%S')} | "
            f"{log.level} | {log.log_type} | {log.source} | {log.position} | {log.message}"
            for log in self.log_buffer
        )


# 实例化日志记录器
logger = Logger()
logger.load_config()
logger.createLogFile()
