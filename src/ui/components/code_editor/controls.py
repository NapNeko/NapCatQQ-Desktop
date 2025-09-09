# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING

# 第三方库导入
from qfluentwidgets import PlainTextEdit
from PySide6.QtCore import QSize
from PySide6.QtGui import QPaintEvent
from PySide6.QtWidgets import QWidget

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.components.code_editor.editor import CodeEditorBase


class LineNumberArea(QWidget):
    """行号区域控件"""

    def __init__(self, editor: "CodeEditorBase") -> None:
        super().__init__(editor)
        self.code_editor = editor

    def sizeHint(self) -> QSize:
        # 返回推荐的尺寸大小
        return QSize(self.code_editor.lineNumberAreaWidth(), 0)

    def paintEvent(self, event: QPaintEvent) -> None:
        # 绘制事件处理，委托给 CodeEditorBase 中的 line_number_area_paint_event 方法
        self.code_editor.line_number_area_paint_event(event)
