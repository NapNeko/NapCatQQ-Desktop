# -*- coding: utf-8 -*-
import re

from PySide6.QtCore import QRegularExpression, Qt
from PySide6.QtGui import QColor, QSyntaxHighlighter, QTextCharFormat, QTextDocument


class LogHighlighter(QSyntaxHighlighter):
    """
    日志高亮器，兼容 NapCat / NCD 常见日志格式。

    支持：
        - `2026-03-20 22:02:45` / `03-20 22:02:45` 时间戳
        - `[info]` / `[ERROR]` 等大小写混合日志级别
        - URL、Windows 路径、引号中的启动命令
    """

    def __init__(self, parent: QTextDocument) -> None:
        super().__init__(parent)

        self.timestamp_format = QTextCharFormat()
        self.timestamp_format.setForeground(QColor("#7f8c98"))

        self.tag_format = QTextCharFormat()
        self.tag_format.setForeground(QColor("#7aa2f7"))

        self.url_format = QTextCharFormat()
        self.url_format.setForeground(QColor("#4fd6be"))

        self.path_format = QTextCharFormat()
        self.path_format.setForeground(QColor("#e0af68"))

        self.command_format = QTextCharFormat()
        self.command_format.setForeground(QColor("#c678dd"))

        self.level_formats: dict[str, QTextCharFormat] = {
            "TRACE": QTextCharFormat(),
            "DEBUG": QTextCharFormat(),
            "INFO": QTextCharFormat(),
            "WARN": QTextCharFormat(),
            "WARNING": QTextCharFormat(),
            "ERROR": QTextCharFormat(),
            "FATAL": QTextCharFormat(),
            "SUCCESS": QTextCharFormat(),
        }
        self.level_formats["TRACE"].setForeground(QColor("#9aa5ce"))
        self.level_formats["DEBUG"].setForeground(QColor("#c0caf5"))
        self.level_formats["INFO"].setForeground(QColor("#73daca"))
        self.level_formats["WARN"].setForeground(QColor("#e0af68"))
        self.level_formats["WARNING"].setForeground(QColor("#e0af68"))
        self.level_formats["ERROR"].setForeground(QColor("#f7768e"))
        self.level_formats["FATAL"].setForeground(QColor("#ff4d6d"))
        self.level_formats["SUCCESS"].setForeground(QColor("#9ece6a"))

        self.timestamp_patterns = [
            QRegularExpression(r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}(?:\.\d{3})?"),
            QRegularExpression(r"^\d{2}-\d{2} \d{2}:\d{2}:\d{2}"),
        ]

        self.level_pattern = QRegularExpression(r"\[(trace|debug|info|warn|warning|error|fatal|success)\]")
        self.level_pattern.setPatternOptions(QRegularExpression.PatternOption.CaseInsensitiveOption)

        self.tag_pattern = QRegularExpression(r"\[(?!trace|debug|info|warn|warning|error|fatal|success)[^\[\]\r\n]+\]")
        self.tag_pattern.setPatternOptions(QRegularExpression.PatternOption.CaseInsensitiveOption)

        self.url_pattern = QRegularExpression(r"https?://[^\s\"']+")
        self.path_pattern = QRegularExpression(r"(?:[A-Za-z]:\\|\\\\)[^\"'\r\n]+")
        self.command_pattern = QRegularExpression(r"\"[^\"]+\"")

    def highlightBlock(self, text: str) -> None:
        """
        高亮当前文本块。
        """
        for pattern in self.timestamp_patterns:
            match = pattern.match(text)
            if match.hasMatch():
                self.setFormat(match.capturedStart(), match.capturedLength(), self.timestamp_format)
                break

        self._apply_pattern(text, self.tag_pattern, self.tag_format)
        self._highlight_levels(text)
        self._apply_pattern(text, self.url_pattern, self.url_format)
        self._highlight_paths(text)
        self._apply_pattern(text, self.command_pattern, self.command_format)

    def _apply_pattern(self, text: str, pattern: QRegularExpression, text_format: QTextCharFormat) -> None:
        """按正则将格式应用到整行的所有匹配。"""
        match_iter = pattern.globalMatch(text)
        while match_iter.hasNext():
            match = match_iter.next()
            self.setFormat(match.capturedStart(), match.capturedLength(), text_format)

    def _highlight_levels(self, text: str) -> None:
        """高亮方括号中的日志级别。"""
        match_iter = self.level_pattern.globalMatch(text)
        while match_iter.hasNext():
            match = match_iter.next()
            level = match.captured(1).upper()
            if level in self.level_formats:
                self.setFormat(match.capturedStart(), match.capturedLength(), self.level_formats[level])

    def _highlight_paths(self, text: str) -> None:
        """高亮 Windows 路径，兼容带空格路径并裁剪掉尾随日志文本。"""
        match_iter = self.path_pattern.globalMatch(text)
        while match_iter.hasNext():
            match = match_iter.next()
            path_text = self._trim_path_match(match.captured())
            if path_text:
                self.setFormat(match.capturedStart(), len(path_text), self.path_format)

    @staticmethod
    def _trim_path_match(path_text: str) -> str:
        """裁剪宽匹配结果，避免把路径后的普通文本一并高亮。"""
        trimmed = path_text.rstrip()

        for delimiter in (r"\s+\{", r"\s+\[", r"\s+\(", r"\s+--"):
            if split_match := re.search(delimiter, trimmed):
                trimmed = trimmed[: split_match.start()].rstrip()

        # 若路径包含明确扩展名，则截断到扩展名结尾，避免吞掉后续说明文字。
        if ext_match := re.search(r"\.[A-Za-z0-9_]{1,8}(?=$|[^A-Za-z0-9_])", trimmed):
            trimmed = trimmed[: ext_match.end()]

        return trimmed


class NCDLogHighlighter(QSyntaxHighlighter):
    """
    NCD 日志高亮器，支持 SUCCESS、DEBUG、INFO、WARN、ERROR 等日志级别。

    时间戳显示深绿色，日志级别显示对应颜色。
    """

    def __init__(self, parent: QTextDocument) -> None:
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

    def __init__(self, document: QTextDocument) -> None:
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
