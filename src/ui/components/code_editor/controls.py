# -*- coding: utf-8 -*-
# 第三方库导入
from qfluentwidgets import PlainTextEdit
from PySide6.QtGui import QPaintEvent
from PySide6.QtCore import QSize
from PySide6.QtWidgets import QWidget


class LineNumberArea(QWidget):
    def __init__(self, editor: PlainTextEdit):
        super().__init__(editor)
        self.code_editor = editor

    def sizeHint(self) -> QSize:
        # 返回推荐的尺寸大小
        return QSize(self.code_editor.lineNumberAreaWidth(), 0)

    def paintEvent(self, event: QPaintEvent) -> None:
        # 绘制事件处理，委托给 CodeEditor 中的 lineNumberAreaPaintEvent 方法
        self.code_editor.lineNumberAreaPaintEvent(event)
