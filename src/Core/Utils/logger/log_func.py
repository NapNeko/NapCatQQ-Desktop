# -*- coding: utf-8 -*-
# 标准库导入
from pathlib import Path
from datetime import datetime
from contextlib import contextmanager

# 项目内模块导入
from src.Core.Utils.logger.log_data import Log, LogGroup, LogPosition
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
        log_group: LogGroup = None,
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
            - log_group: LogGroup - 可选，若指定则将日志添加到日志组
        """
        # 构造 Log
        log = Log(level, message, time, log_type, log_source, log_position)

        if log_group:
            # 如果提供了 log_group，将日志添加到它的内部
            log_group.add(log)
        else:
            # 否则直接添加到 log_buffer
            self.log_buffer.append(log)

        # 遍历日志列表, 追加到日志文件中
        with open(self.log_path, "a", encoding="utf-8") as f:
            f.write(log.toString() + "\n")
        # 判断是否需要清理缓冲区
        self.clearBuffer()
        # 打印 log
        print(log)

    @capture_call_location
    def debug(
        self,
        message: str,
        log_type: LogType = LogType.NONE_TYPE,
        log_source: LogSource = LogSource.NONE,
        log_position: LogPosition = None,
        log_group: LogGroup = None,
    ):
        self._log(LogLevel.DBUG, message, datetime.now().timestamp(), log_type, log_source, log_position, log_group)

    @capture_call_location
    def info(
        self,
        message: str,
        log_type: LogType = LogType.NONE_TYPE,
        log_source: LogSource = LogSource.NONE,
        log_position: LogPosition = None,
        log_group: LogGroup = None,
    ):
        self._log(LogLevel.INFO, message, datetime.now().timestamp(), log_type, log_source, log_position, log_group)

    @capture_call_location
    def warning(
        self,
        message: str,
        log_type: LogType = LogType.NONE_TYPE,
        log_source: LogSource = LogSource.NONE,
        log_position: LogPosition = None,
        log_group: LogGroup = None,
    ):
        self._log(LogLevel.WARN, message, datetime.now().timestamp(), log_type, log_source, log_position, log_group)

    @capture_call_location
    def error(
        self,
        message: str,
        log_type: LogType = LogType.NONE_TYPE,
        log_source: LogSource = LogSource.NONE,
        log_position: LogPosition = None,
        log_group: LogGroup = None,
    ):
        self._log(LogLevel.EROR, message, datetime.now().timestamp(), log_type, log_source, log_position, log_group)

    @contextmanager
    def group(self, name: str, log_type: LogType, log_source: LogSource):
        """创建一个带有开始/结束标记的逻辑日志组"""
        log_group = LogGroup(name, log_type, log_source)
        try:
            self.info(
                f"{'-' * 20} > {name} 开始 < {'-' * 20}",
                log_type=log_type, log_source=log_source, log_group=log_group
            )
            yield log_group
        finally:
            self.info(
                f"{'-' * 20} > {name} 结束 < {'-' * 20}",
                log_type=log_type, log_source=log_source, log_group=log_group
            )
            self.log_buffer.append(log_group)


# 实例化日志记录器
logger = Logger()
logger.load_config()
logger.createLogFile()
