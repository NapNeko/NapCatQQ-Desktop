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
        # 绘制行号区域的内容
        painter = QPainter(self.line_number_area)
        painter.setPen(Qt.GlobalColor.gray)  # 设置画笔颜色为灰色
        painter.fillRect(event.rect(), Qt.GlobalColor.transparent)  # 使用透明背景

        block = self.firstVisibleBlock()
        block_number = block.blockNumber()
        offset = self.contentOffset()
        top = self.blockBoundingGeometry(block).translated(offset).top()
        bottom = top + self.blockBoundingRect(block).height()
        content_left = self.contentsRect().left()  # 内容区域的左边界

        while block.isValid() and top <= event.rect().bottom():
            if block.isVisible() and bottom >= event.rect().top():
                line_number = str(block_number + 1)
                painter.drawText(
                    QRectF(0, top, self.line_number_area.width() - 2, self.fontMetrics().height()),
                    Qt.AlignmentFlag.AlignRight,
                    line_number,
                )

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
