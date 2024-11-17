# -*- coding: utf-8 -*-
# 标准库导入

# 第三方库导入
from qfluentwidgets import PlainTextEdit
from PySide6.QtGui import QFontDatabase

# 项目内模块导入
from src.Core.Utils.singleton import singleton
from src.Core.Utils.logger.log_enum import LogLevel


@singleton
class LoggerEdit(PlainTextEdit):
    """日志输出控件"""

    level: LogLevel

    def __init__(self, parent=None):
        super().__init__(parent)

        # 日志等级

        # 初始化控件
        self.initWidget()

    def initWidget(self) -> None:
        """初始化控件"""
        self.setReadOnly(True)
        self.setFont(QFontDatabase.systemFont(QFontDatabase.SystemFont.FixedFont).pixelSize(10))
