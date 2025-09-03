# -*- coding: utf-8 -*-

# 标准库导入
import json
from typing import Optional

# 第三方库导入
from qfluentwidgets import FluentIcon, TeachingTip, PlainTextEdit
from PySide6.QtGui import QPen, QFont, QColor, QPainter, QKeyEvent, QPaintEvent, QTextCursor, QTextFormat
from PySide6.QtCore import Qt, Slot, QRect, QRectF
from PySide6.QtWidgets import QTextEdit, QApplication

# 项目内模块导入
from src.ui.style_sheet import StyleSheet
from src.core.utils.logger import logger
from src.ui.common.code_editor.controls import LineNumberArea
from src.ui.common.code_editor.highlight import JsonHighlighter


class CodeEditorBase(PlainTextEdit):
    """
    基础代码编辑器

    功能：
        - 显示行号
        - 光标所在行高亮
        - Shift 多行选择并高亮
        - 行号高亮同步
    """

    INDENT: int = 4  # 缩进宽度

    def __init__(self, parent: Optional[PlainTextEdit] = None) -> None:
        super().__init__(parent)
        # 变量
        self.font_size: int = 12

        # 行号区域
        self.line_number_area = LineNumberArea(self)

        # 绑定信号
        self.blockCountChanged.connect(lambda _: self.updateLineNumberAreaWidth())
        self.updateRequest.connect(self.updateLineNumberArea)
        self.cursorPositionChanged.connect(self.handleCursorPositionChanged)

        # 编辑器基础设置
        self.setReadOnly(False)
        self.setFontSize(12)
        self.updateLineNumberAreaWidth()

        # 当前行与选中行信息
        self.current_line: int = -1
        self.selection_start_line: int = -1
        self.selected_lines: set[int] = set()

        # 样式
        StyleSheet.CODE_EDITOR.apply(self)

    def setFontSize(self, size: int) -> None:
        """设置编辑器字体大小"""
        self.font_size = size
        self.setFont(QFont(self.font().families(), self.font_size))

    def lineNumberAreaWidth(self) -> int:
        """计算行号区域宽度"""
        max_block = max(1, self.blockCount())
        digits = len(str(max_block))
        return 2 + self.fontMetrics().horizontalAdvance("9") * digits

    def lineNumberAreaPaintEvent(self, event: QPaintEvent) -> None:
        """绘制行号区域"""
        painter = QPainter(self.line_number_area)
        painter.setFont(self.font())
        painter.fillRect(event.rect(), Qt.GlobalColor.transparent)

        block = self.firstVisibleBlock()
        block_number = block.blockNumber()
        offset = self.contentOffset()
        top = self.blockBoundingGeometry(block).translated(offset).top()
        bottom = top + self.blockBoundingRect(block).height()
        area_width = self.line_number_area.width()
        text_height = self.fontMetrics().height()
        text_margin = 2

        while block.isValid() and top <= event.rect().bottom():
            if block.isVisible() and bottom >= event.rect().top():
                line_number = str(block_number + 1)
                rect = QRectF(0, top, area_width - text_margin, text_height)
                painter.setPen(
                    QColor(255, 255, 255)
                    if block_number == self.current_line or block_number in self.selected_lines
                    else Qt.GlobalColor.gray
                )
                painter.drawText(rect, Qt.AlignmentFlag.AlignRight, line_number)
            block = block.next()
            top = bottom
            bottom = top + self.blockBoundingRect(block).height()
            block_number += 1

    def resizeEvent(self, event) -> None:
        """更新行号区域位置"""
        super().resizeEvent(event)
        content_rect = self.contentsRect()
        margin = 8
        rect = QRect(
            content_rect.left() + margin,
            content_rect.top(),
            self.lineNumberAreaWidth(),
            content_rect.height(),
        )
        self.line_number_area.setGeometry(rect)

    @Slot()
    def updateLineNumberAreaWidth(self) -> None:
        """更新行号区域宽度"""
        self.setViewportMargins(self.lineNumberAreaWidth() + 10, 0, 0, 0)

    @Slot(QRect, int)
    def updateLineNumberArea(self, rect: QRect, dy: int) -> None:
        """滚动或更新行号区域"""
        if dy:
            self.line_number_area.scroll(0, dy)
        else:
            self.line_number_area.update(0, rect.y(), self.line_number_area.width(), rect.height())
        if rect.contains(self.viewport().rect()):
            self.updateLineNumberAreaWidth()

    def setPlainText(self, text: str) -> None:
        """保留滚动位置设置文本"""
        scroll_pos = self.verticalScrollBar().value()
        super().setPlainText(text)
        QApplication.processEvents()
        self.verticalScrollBar().setValue(scroll_pos)

    def mousePressEvent(self, event) -> None:
        """处理鼠标点击多行选择"""
        super().mousePressEvent(event)
        cursor = self.cursorForPosition(event.position().toPoint())
        block_number = cursor.block().blockNumber()
        modifiers = QApplication.keyboardModifiers()

        if modifiers & Qt.KeyboardModifier.ShiftModifier and self.selection_start_line != -1:
            start = min(self.selection_start_line, block_number)
            end = max(self.selection_start_line, block_number)
            self.selected_lines = set(range(start, end + 1))
        else:
            self.selected_lines = {block_number}
            self.selection_start_line = block_number

        self.current_line = block_number
        self.highlightSelectedLines()
        self.line_number_area.update()

    def handleCursorPositionChanged(self) -> None:
        """光标移动处理多行选择与高亮"""
        cursor = self.textCursor()
        line_number = cursor.block().blockNumber()
        self.current_line = line_number
        modifiers = QApplication.keyboardModifiers()

        if modifiers & Qt.KeyboardModifier.ShiftModifier and self.selection_start_line != -1:
            start = min(self.selection_start_line, line_number)
            end = max(self.selection_start_line, line_number)
            self.selected_lines = set(range(start, end + 1))
        else:
            self.selected_lines = {line_number}
            self.selection_start_line = line_number

        self.highlightSelectedLines()
        self.line_number_area.update()
        self.viewport().update()

    def highlightSelectedLines(self) -> None:
        """高亮当前选中行或多行"""
        extra_selections = []
        for line_number in self.selected_lines:
            selection = QTextEdit.ExtraSelection()
            selection.format.setBackground(QColor(255, 255, 255, 25))
            selection.format.setProperty(QTextFormat.FullWidthSelection, True)
            cursor = self.textCursor()
            cursor.setPosition(self.document().findBlockByNumber(line_number).position())
            selection.cursor = cursor
            extra_selections.append(selection)
        self.setExtraSelections(extra_selections)


class CodeEditor(CodeEditorBase):
    """
    代码编辑器基类

    功能：
        - 自动补全括号/引号
        - 智能缩进与回车
        - Ctrl+Enter 继承缩进
    """

    AUTO_COMPLETE_CHARS: dict[str, str] = {"{": "}", "[": "]", "(": ")", '"': '"', "'": "'"}
    INDENT: int = 4

    def __init__(self, parent: Optional[PlainTextEdit] = None) -> None:
        super().__init__(parent)
        self.setLineWrapMode(PlainTextEdit.LineWrapMode.NoWrap)
        self.setTabChangesFocus(False)

    def keyPressEvent(self, event: QKeyEvent) -> None:
        """处理按键事件"""
        cursor = self.textCursor()
        text = self.toPlainText()

        if event.key() == Qt.Key.Key_Backspace:
            if self.handle_backspace(cursor, text) or self.handle_indent_backspace(cursor, text):
                return
        elif event.key() == Qt.Key.Key_Tab:
            cursor.insertText(" " * self.INDENT)
            self.setTextCursor(cursor)
            return
        elif event.key() in (Qt.Key.Key_Return, Qt.Key.Key_Enter):
            if event.modifiers() & Qt.KeyboardModifier.ControlModifier:
                if self.handle_ctrl_return(cursor, text):
                    return
            else:
                if self.handle_return(cursor, text):
                    return
        elif event.text() in self.AUTO_COMPLETE_CHARS:
            self.handle_auto_complete(cursor, event.text())
            return
        elif self.should_skip_closing_character(cursor, event.text(), text):
            cursor.movePosition(QTextCursor.MoveOperation.Right)
            self.setTextCursor(cursor)
            return

        super().keyPressEvent(event)

    def handle_backspace(self, cursor: QTextCursor, text: str) -> bool:
        """退格处理自动删除配对字符"""
        pos = cursor.position()
        if 0 < pos < len(text):
            prev_char, next_char = text[pos - 1], text[pos]
            for open_char, close_char in self.AUTO_COMPLETE_CHARS.items():
                if prev_char == open_char and next_char == close_char:
                    cursor.beginEditBlock()
                    cursor.deletePreviousChar()
                    cursor.deleteChar()
                    cursor.endEditBlock()
                    self.setTextCursor(cursor)
                    return True
        return False

    def handle_indent_backspace(self, cursor: QTextCursor, text: str) -> bool:
        """退格删除缩进空格"""
        pos = cursor.position()
        if pos == 0:
            return False
        line_start = text.rfind("\n", 0, pos) + 1
        col = pos - line_start
        if col > 0:
            cursor.movePosition(QTextCursor.Left, QTextCursor.KeepAnchor, min(self.INDENT, col))
            if cursor.selectedText().isspace():
                cursor.removeSelectedText()
                self.setTextCursor(cursor)
                return True
        return False

    def handle_return(self, cursor: QTextCursor, text: str) -> bool:
        """智能回车处理括号缩进"""
        pos = cursor.position()
        line_start = text.rfind("\n", 0, pos) + 1
        line_text = text[line_start:pos]
        indent = len(line_text) - len(line_text.lstrip())

        cursor.beginEditBlock()
        if 0 < pos < len(text):
            prev_char, next_char = text[pos - 1], text[pos]
            if prev_char in self.AUTO_COMPLETE_CHARS and self.AUTO_COMPLETE_CHARS[prev_char] == next_char:
                cursor.deleteChar()
                cursor.insertText("\n" + " " * (indent + self.INDENT))
                cursor.insertText("\n" + " " * indent + next_char)
                cursor.movePosition(QTextCursor.Up)
                cursor.movePosition(QTextCursor.EndOfLine)
                cursor.endEditBlock()
                self.setTextCursor(cursor)
                return True

        cursor.insertText("\n" + " " * indent)
        cursor.endEditBlock()
        self.setTextCursor(cursor)
        return True

    def handle_ctrl_return(self, cursor: QTextCursor, text: str) -> bool:
        """Ctrl+Enter 在下一行继承缩进"""
        pos = cursor.position()
        line_start = text.rfind("\n", 0, pos) + 1
        line_end = text.find("\n", pos)
        if line_end == -1:
            line_end = len(text)
        line_text = text[line_start:line_end]
        indent = len(line_text) - len(line_text.lstrip())

        cursor.beginEditBlock()
        cursor.setPosition(line_end)
        cursor.insertText("\n" + " " * indent)
        cursor.endEditBlock()
        self.setTextCursor(cursor)
        return True

    def handle_auto_complete(self, cursor: QTextCursor, char: str) -> None:
        """处理自动补全括号或引号"""
        close_char = self.AUTO_COMPLETE_CHARS[char]
        cursor.beginEditBlock()
        cursor.insertText(char + close_char)
        cursor.movePosition(QTextCursor.MoveOperation.Left)
        cursor.endEditBlock()
        self.setTextCursor(cursor)

    def should_skip_closing_character(self, cursor: QTextCursor, input_char: str, text: str) -> bool:
        """检查光标后是否已有闭合字符，需要跳过"""
        pos = cursor.position()
        return pos < len(text) and input_char in self.AUTO_COMPLETE_CHARS.values() and text[pos] == input_char


class JsonEditor(CodeEditor):
    """JSON 专用编辑器，带语法高亮, 层级辅助线"""

    def __init__(self, parent: Optional[PlainTextEdit] = None) -> None:
        super().__init__(parent)
        self.highlighter = JsonHighlighter(self.document())

        # 链接信号
        self.cursorPositionChanged.connect(lambda: self.viewport().update())
        self.updateRequest.connect(lambda rect, dy: self.viewport().update())

    def setJson(self, json_data: str) -> None:
        """设置 JSON 数据"""
        try:
            parsed = json.loads(json_data)
            pretty_json = json.dumps(parsed, indent=4, ensure_ascii=False)
            self.setPlainText(pretty_json)
        except json.JSONDecodeError as e:
            logger.error(f"无效的 JSON 数据: {e}")
            self.setPlainText(json_data)

    def getJson(self) -> str:
        """获取当前 JSON 数据, 压缩转为一行字符串"""
        try:
            parsed = json.loads(self.toPlainText())
            return json.dumps(parsed, ensure_ascii=False)
        except json.JSONDecodeError as e:
            logger.error(f"无效的 JSON 数据: {e}")
            return self.toPlainText()

    def checkJson(self, tips: bool = True) -> bool:
        """检查当前文本是否为有效 JSON, 并标记错误行"""
        self.error_line = -1  # 清除旧错误
        try:
            json.loads(self.toPlainText())
            self.setExtraSelections([])

            if tips:
                TeachingTip.create(
                    target=self.parent(),
                    title="JSON 语法正确!",
                    content="校验完成, JSON 语法正确！",
                    icon=FluentIcon.ACCEPT,
                    duration=2000,
                    parent=self,
                )

            self.setPlainText(json.dumps(json.loads(self.toPlainText()), indent=4, ensure_ascii=False))

            return True

        except json.JSONDecodeError as e:
            self.error_line = e.lineno - 1
            self.highlightErrorLine(self.error_line)
            logger.error(f"JSON 语法错误: {e}")

            if tips:
                TeachingTip.create(
                    target=self.parent(),
                    title="JSON 语法错误!",
                    content=f"校验完成, JSON 语法错误! 请检查高亮行上下几行\n {e}",
                    icon=FluentIcon.CLOSE,
                    duration=-1,
                    parent=self,
                )

            return False

    def highlightErrorLine(self, line_number: int) -> None:
        """高亮并跳转到指定错误行"""
        block = self.document().findBlockByNumber(line_number)
        if not block.isValid():
            return

        cursor = self.textCursor()
        cursor.setPosition(block.position())
        self.setTextCursor(cursor)
        self.centerCursor()

        extra = QTextEdit.ExtraSelection()
        fmt = extra.format
        fmt.setBackground(QColor(255, 0, 0, 50))
        fmt.setProperty(QTextFormat.FullWidthSelection, True)
        extra.cursor = cursor

        self.setExtraSelections([extra])

    def paintEvent(self, event: QPaintEvent) -> None:
        """绘制文本及多层级缩进辅助线"""
        super().paintEvent(event)

        painter = QPainter(self.viewport())
        char_width = self.fontMetrics().horizontalAdvance(" ")
        normal_pen = QPen(QColor(200, 200, 200, 26))
        normal_pen.setWidth(1)
        highlight_pen = QPen(QColor(200, 200, 200, 128))
        highlight_pen.setWidth(2)

        viewport_bottom = self.viewport().height()

        # 记录每个层级的线段范围
        level_ranges: dict[int, list[tuple[int, int]]] = {}
        stack: list[tuple[int, int]] = []
        block_iter = self.document().firstBlock()
        line_number = 0
        while block_iter.isValid():
            line_text = block_iter.text()
            for char in line_text:
                if char in "{[":
                    stack.append((len(stack), line_number))
                elif char in "}]":
                    if stack:
                        level, start = stack.pop()
                        end_line = max(line_number - 1, start)
                        level_ranges.setdefault(level, []).append((start, end_line))
            block_iter = block_iter.next()
            line_number += 1

        # 收集可见行信息
        block = self.firstVisibleBlock()
        top = self.blockBoundingGeometry(block).translated(self.contentOffset()).top()
        bottom = top + self.blockBoundingRect(block).height()
        visible_blocks = []
        while block.isValid() and top <= viewport_bottom:
            visible_blocks.append((block.blockNumber(), top, bottom))
            block = block.next()
            top = bottom
            bottom = top + self.blockBoundingRect(block).height()

        # 绘制普通层级线
        for level, ranges in level_ranges.items():
            x = level * self.INDENT * char_width + 4
            for start, end in ranges:
                for line_number, top, bottom in visible_blocks:
                    if start + 1 <= line_number <= end:
                        painter.setPen(normal_pen)
                        painter.drawLine(x, top, x, bottom)

        # 绘制光标所在层级高亮线
        cursor_line = self.textCursor().block().blockNumber()
        for level, ranges in level_ranges.items():
            for start, end in ranges:
                if start < cursor_line <= end:
                    x = level * self.INDENT * char_width + 4
                    for line_number, top, bottom in visible_blocks:
                        if start + 1 <= line_number <= end:
                            painter.setPen(highlight_pen)
                            painter.drawLine(x, top, x, bottom)
