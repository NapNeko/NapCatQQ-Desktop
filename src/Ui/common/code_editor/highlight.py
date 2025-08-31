# -*- coding: utf-8 -*-
from PySide6.QtGui import QColor, QTextCharFormat, QSyntaxHighlighter
from PySide6.QtCore import Qt, QRegularExpression


class LogHighlighter(QSyntaxHighlighter):
    def __init__(self, parent=None) -> None:
        super().__init__(parent)

        # 初始化不同日志级别的文本格式
        self.formats = {
            "DEBUG": QTextCharFormat(),
            "INFO": QTextCharFormat(),
            "WARN": QTextCharFormat(),
            "ERROR": QTextCharFormat(),
        }

        # 设置每个日志级别的前景色
        self.formats["DEBUG"].setForeground(QColor(Qt.GlobalColor.darkRed))
        self.formats["INFO"].setForeground(QColor(Qt.GlobalColor.green))
        self.formats["WARN"].setForeground(QColor(Qt.GlobalColor.darkYellow))
        self.formats["ERROR"].setForeground(QColor(Qt.GlobalColor.red))

        # 定义日志级别的顺序
        self.log_levels = ["DEBUG", "INFO", "WARN", "ERROR"]

        # 正则表达式模式，用于匹配像 [DEBUG]、[INFO] 等日志级别标签
        self.pattern = QRegularExpression(r"^(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}) \[(DEBUG|INFO|WARN|ERROR)\]")

    def highlightBlock(self, text) -> None:
        # 创建一个格式化对象，用于应用整行的前景色
        text_format = QTextCharFormat()

        # 检查当前文本块是否匹配日志级别的模式
        match = self.pattern.match(text)
        if not match or not match.hasMatch():
            return

        # 获取时间戳的起始位置和长度
        timestamp_start = match.capturedStart(1)
        timestamp_length = match.capturedLength(1)

        # 获取日志级别的起始位置和长度
        log_level_start = match.capturedStart(2)
        log_level_length = match.capturedLength(2)

        # 应用时间戳的格式
        text_format.setForeground(QColor(Qt.GlobalColor.lightGray))
        self.setFormat(timestamp_start, timestamp_length, text_format)

        # 检查捕获的日志级别是否在预定义列表中
        log_level = match.captured(2)
        if log_level in self.log_levels:
            text_format.setForeground(self.formats[log_level].foreground().color())
            self.setFormat(log_level_start, log_level_length, text_format)


class NCDLogHighlighter(QSyntaxHighlighter):
    def __init__(self, parent=None) -> None:
        super().__init__(parent)

        # 初始化不同日志级别的文本格式
        self.formats = {
            "SUCCESS": QTextCharFormat(),
            "DEBUG": QTextCharFormat(),
            "INFO": QTextCharFormat(),
            "WARN": QTextCharFormat(),
            "ERROR": QTextCharFormat(),
        }

        # 设置每个日志级别的前景色
        self.formats["SUCCESS"].setForeground(QColor(Qt.GlobalColor.darkGreen))
        self.formats["DEBUG"].setForeground(QColor(Qt.GlobalColor.darkRed))
        self.formats["INFO"].setForeground(QColor(Qt.GlobalColor.darkBlue))
        self.formats["WARN"].setForeground(QColor(Qt.GlobalColor.darkYellow))
        self.formats["ERROR"].setForeground(QColor(Qt.GlobalColor.red))

        # 定义日志级别的顺序
        self.log_levels = ["SUCCESS", "DEBUG", "INFO", "WARN", "ERROR"]

        # 正则表达式模式，用于匹配日志的时间和日志级别
        self.pattern = QRegularExpression(
            r"^(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}) \| (SUCCESS|DEBUG|INFO|WARN|ERROR) \|"
        )

    def highlightBlock(self, text) -> None:
        # 创建一个格式化对象，用于应用整行的前景色
        text_format = QTextCharFormat()

        # 检查当前文本块是否匹配日志级别的模式
        match = self.pattern.match(text)
        if not match or not match.hasMatch():
            return

        # 获取时间戳的起始位置和长度
        timestamp_start = match.capturedStart(1)
        timestamp_length = match.capturedLength(1)

        # 获取日志级别的起始位置和长度
        log_level_start = match.capturedStart(2)
        log_level_length = match.capturedLength(2)

        # 应用时间戳的格式
        text_format.setForeground(QColor(Qt.GlobalColor.darkGreen))
        self.setFormat(timestamp_start, timestamp_length, text_format)

        # 检查捕获的日志级别是否在预定义列表中
        log_level = match.captured(2)
        if log_level in self.log_levels:
            text_format.setForeground(self.formats[log_level].foreground().color())
            self.setFormat(log_level_start, log_level_length, text_format)
