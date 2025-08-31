# -*- coding: utf-8 -*-

# 第三方库导入
from qfluentwidgets import PlainTextEdit
from PySide6.QtGui import QPainter, QFontDatabase
from PySide6.QtCore import Qt, Slot, QRect, QRectF
from PySide6.QtWidgets import QApplication

# 项目内模块导入
from src.Ui.common.code_editor.controls import LineNumberArea


class CodeEditor(PlainTextEdit):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.line_number_area = LineNumberArea(self)

        # 连接信号和槽函数
        self.blockCountChanged.connect(lambda _: self.updateLineNumberAreaWidth())
        self.updateRequest.connect(self.updateLineNumberArea)

        # 初始设置
        self.setReadOnly(False)
        self.updateLineNumberAreaWidth()
        self.setMonospaceFont()

    def setMonospaceFont(self) -> None:
        """
        设置字体为等宽字体
        """
        font = QFontDatabase.systemFont(QFontDatabase.SystemFont.FixedFont)
        font.setPointSize(10)
        self.setFont(font)

    def lineNumberAreaWidth(self) -> int:
        """
        计算行号区域的宽度
        """
        max_num = max(1, self.blockCount())
        digits = len(str(max_num))  # 自动算需要多少位
        space = 2 + self.fontMetrics().horizontalAdvance("9") * digits
        return space

    def lineNumberAreaPaintEvent(self, event) -> None:
        """绘制行号区域的内容"""
        painter = QPainter(self.line_number_area)
        painter.setPen(Qt.GlobalColor.gray)

        # 使用透明背景填充整个区域
        painter.fillRect(event.rect(), Qt.GlobalColor.transparent)

        # 获取第一个可见文本块及其相关信息
        block = self.firstVisibleBlock()
        block_number = block.blockNumber()
        content_offset = self.contentOffset()

        # 计算第一个文本块的顶部和底部位置
        block_geometry = self.blockBoundingGeometry(block).translated(content_offset)
        top = block_geometry.top()
        bottom = top + self.blockBoundingRect(block).height()

        # 准备绘制参数
        area_width = self.line_number_area.width()
        text_margin = 2  # 右边距
        text_rect_width = area_width - text_margin
        text_height = self.fontMetrics().height()

        # 遍历所有可见文本块并绘制行号
        while block.isValid() and top <= event.rect().bottom():
            # 只绘制在当前视图区域内可见的文本块
            if block.isVisible() and bottom >= event.rect().top():
                line_number = str(block_number + 1)

                # 创建文本绘制区域
                text_rect = QRectF(0, top, text_rect_width, text_height)

                # 绘制行号（右对齐）
                painter.drawText(text_rect, Qt.AlignmentFlag.AlignRight, line_number)

            # 移动到下一个文本块
            block = block.next()
            top = bottom
            bottom = top + self.blockBoundingRect(block).height()
            block_number += 1

    def resizeEvent(self, event) -> None:
        super().resizeEvent(event)
        # 调整行号区域的位置和大小
        content_rect = self.contentsRect()
        area_width = self.lineNumberAreaWidth()
        rect = QRect(content_rect.left(), content_rect.top(), area_width, content_rect.height())
        self.line_number_area.setGeometry(rect)

    def setPlainText(self, text):
        scroll_position = self.verticalScrollBar().value()
        super().setPlainText(text)
        QApplication.processEvents()
        self.verticalScrollBar().setValue(scroll_position)

    @Slot(int)
    def updateLineNumberAreaWidth(self) -> None:
        # 更新行号区域的宽度
        self.setViewportMargins(self.lineNumberAreaWidth() + 10, 0, 0, 0)

    @Slot(QRect, int)
    def updateLineNumberArea(self, rect: QRect, dy: int) -> None:
        # 更新行号区域的显示内容
        if dy:
            self.line_number_area.scroll(0, dy)
        else:
            width = self.line_number_area.width()
            self.line_number_area.update(0, rect.y(), width, rect.height())

        if rect.contains(self.viewport().rect()):
            self.updateLineNumberAreaWidth()
