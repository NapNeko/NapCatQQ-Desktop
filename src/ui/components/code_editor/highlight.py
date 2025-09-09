# -*- coding: utf-8 -*-
# 标准库导入
from typing import Optional

from PySide6.QtCore import QRegularExpression, Qt
from PySide6.QtGui import QColor, QSyntaxHighlighter, QTextCharFormat


class LogHighlighter(QSyntaxHighlighter):
    """
    日志高亮器，支持 DEBUG、INFO、WARN、ERROR 等日志级别高亮。

    时间戳显示为浅灰色，日志级别显示对应颜色。
    """

    def __init__(self, parent: Optional[QTextCharFormat] = None) -> None:
        super().__init__(parent)

        # 初始化不同日志级别的文本格式
        self.formats: dict[str, QTextCharFormat] = {
            "DEBUG": QTextCharFormat(),
            "INFO": QTextCharFormat(),
            "WARN": QTextCharFormat(),
            "ERROR": QTextCharFormat(),
        }

        # 设置日志级别前景色
        self.formats["DEBUG"].setForeground(QColor(Qt.GlobalColor.darkRed))
        self.formats["INFO"].setForeground(QColor(Qt.GlobalColor.green))
        self.formats["WARN"].setForeground(QColor(Qt.GlobalColor.darkYellow))
        self.formats["ERROR"].setForeground(QColor(Qt.GlobalColor.red))

        # 日志级别顺序
        self.log_levels = ["DEBUG", "INFO", "WARN", "ERROR"]

        # 匹配日志时间戳和日志级别的正则表达式
        self.pattern = QRegularExpression(r"^(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}) \[(DEBUG|INFO|WARN|ERROR)\]")

    def highlightBlock(self, text: str) -> None:
        """
        高亮当前文本块。

        时间戳显示浅灰色，日志级别显示对应颜色。
        """
        text_format = QTextCharFormat()
        match = self.pattern.match(text)
        if not match.hasMatch():
            return

        # 高亮时间戳
        timestamp_start = match.capturedStart(1)
        timestamp_length = match.capturedLength(1)
        text_format.setForeground(QColor(Qt.GlobalColor.lightGray))
        self.setFormat(timestamp_start, timestamp_length, text_format)

        # 高亮日志级别
        log_level = match.captured(2)
        if log_level in self.log_levels:
            log_level_start = match.capturedStart(2)
            log_level_length = match.capturedLength(2)
            text_format.setForeground(self.formats[log_level].foreground().color())
            self.setFormat(log_level_start, log_level_length, text_format)


class NCDLogHighlighter(QSyntaxHighlighter):
    """
    NCD 日志高亮器，支持 SUCCESS、DEBUG、INFO、WARN、ERROR 等日志级别。

    时间戳显示深绿色，日志级别显示对应颜色。
    """

    def __init__(self, parent: Optional[QTextCharFormat] = None) -> None:
        super().__init__(parent)

        self.formats: dict[str, QTextCharFormat] = {
            "SUCCESS": QTextCharFormat(),
            "DEBUG": QTextCharFormat(),
            "INFO": QTextCharFormat(),
            "WARN": QTextCharFormat(),
            "ERROR": QTextCharFormat(),
        }

        # 设置日志级别前景色
        self.formats["SUCCESS"].setForeground(QColor(Qt.GlobalColor.darkGreen))
        self.formats["DEBUG"].setForeground(QColor(Qt.GlobalColor.darkRed))
        self.formats["INFO"].setForeground(QColor(Qt.GlobalColor.darkBlue))
        self.formats["WARN"].setForeground(QColor(Qt.GlobalColor.darkYellow))
        self.formats["ERROR"].setForeground(QColor(Qt.GlobalColor.red))

        self.log_levels = ["SUCCESS", "DEBUG", "INFO", "WARN", "ERROR"]

        # 匹配时间戳、日志级别的正则
        self.pattern = QRegularExpression(
            r"^(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}) \| (SUCCESS|DEBUG|INFO|WARN|ERROR) \|"
        )

    def highlightBlock(self, text: str) -> None:
        """
        高亮当前文本块。

        时间戳显示深绿色，日志级别显示对应颜色。
        """
        text_format = QTextCharFormat()
        match = self.pattern.match(text)
        if not match.hasMatch():
            return

        # 高亮时间戳
        timestamp_start = match.capturedStart(1)
        timestamp_length = match.capturedLength(1)
        text_format.setForeground(QColor(Qt.GlobalColor.darkGreen))
        self.setFormat(timestamp_start, timestamp_length, text_format)

        # 高亮日志级别
        log_level = match.captured(2)
        if log_level in self.log_levels:
            log_level_start = match.capturedStart(2)
            log_level_length = match.capturedLength(2)
            text_format.setForeground(self.formats[log_level].foreground().color())
            self.setFormat(log_level_start, log_level_length, text_format)


class JsonHighlighter(QSyntaxHighlighter):
    """
    JSON 高亮器

    支持：
        - 键名
        - 冒号
        - 字符串值
        - 数字
        - 布尔值与 null
        - 逗号
        - 大括号和中括号
    """

    def __init__(self, document) -> None:
        super().__init__(document)

        # 配色
        self.format_key = QTextCharFormat()
        self.format_key.setForeground(QColor("#8aadf4"))

        self.format_string_value = QTextCharFormat()
        self.format_string_value.setForeground(QColor("#a6e3a1"))

        self.format_number = QTextCharFormat()
        self.format_number.setForeground(QColor("#f5c2e7"))

        self.format_boolean = QTextCharFormat()
        self.format_boolean.setForeground(QColor("#f38ba8"))

        self.format_colon = QTextCharFormat()
        self.format_colon.setForeground(QColor("#cad3f5"))

        self.format_comma = QTextCharFormat()
        self.format_comma.setForeground(QColor("#f38ba8"))

        self.format_braces = QTextCharFormat()
        self.format_braces.setForeground(QColor("#f5c2e7"))

        # 正则
        self.regex_string = QRegularExpression(r'"([^"\\]*(\\.[^"\\]*)*)"')
        self.regex_number = QRegularExpression(r"\b-?\d+(\.\d+)?([eE][+-]?\d+)?\b")
        self.regex_boolean = QRegularExpression(r"\b(true|false|null)\b")
        self.regex_colon = QRegularExpression(r":")
        self.regex_comma = QRegularExpression(r",")
        self.regex_braces = QRegularExpression(r"[\{\}\[\]]")

    def highlightBlock(self, text: str) -> None:
        string_ranges = []

        # --- 先匹配字符串 ---
        it = self.regex_string.globalMatch(text)
        while it.hasNext():
            match = it.next()
            start = match.capturedStart()
            length = match.capturedLength()
            string_ranges.append((start, start + length))
            # 判断是否是键名：后面跟冒号
            after_string = text[start + length :].lstrip()
            if after_string.startswith(":"):
                self.setFormat(start, length, self.format_key)  # 键名
            else:
                self.setFormat(start, length, self.format_string_value)  # 字符串值

        def in_string(pos):
            return any(start <= pos < end for start, end in string_ranges)

        # --- 高亮数字 ---
        it = self.regex_number.globalMatch(text)
        while it.hasNext():
            match = it.next()
            if not in_string(match.capturedStart()):
                self.setFormat(match.capturedStart(), match.capturedLength(), self.format_number)

        # --- 高亮布尔值和 null ---
        it = self.regex_boolean.globalMatch(text)
        while it.hasNext():
            match = it.next()
            if not in_string(match.capturedStart()):
                self.setFormat(match.capturedStart(), match.capturedLength(), self.format_boolean)

        # --- 高亮冒号 ---
        it = self.regex_colon.globalMatch(text)
        while it.hasNext():
            match = it.next()
            if not in_string(match.capturedStart()):
                self.setFormat(match.capturedStart(), 1, self.format_colon)

        # --- 高亮逗号 ---
        it = self.regex_comma.globalMatch(text)
        while it.hasNext():
            match = it.next()
            if not in_string(match.capturedStart()):
                self.setFormat(match.capturedStart(), 1, self.format_comma)

        # --- 高亮括号 ---
        it = self.regex_braces.globalMatch(text)
        while it.hasNext():
            match = it.next()
            if not in_string(match.capturedStart()):
                self.setFormat(match.capturedStart(), 1, self.format_braces)
