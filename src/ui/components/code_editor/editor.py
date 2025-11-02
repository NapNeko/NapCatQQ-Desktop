# -*- coding: utf-8 -*-

# 标准库导入
import json
from typing import Optional

# 第三方库导入
from qfluentwidgets import FluentIcon, PlainTextEdit, TeachingTip, isDarkTheme
from PySide6.QtCore import QRect, QRectF, Qt, Signal
from PySide6.QtGui import (
    QColor,
    QKeyEvent,
    QMouseEvent,
    QPainter,
    QPaintEvent,
    QPen,
    QResizeEvent,
    QTextCursor,
    QTextFormat,
)
from PySide6.QtWidgets import QApplication, QPlainTextEdit, QTextEdit, QWidget

# 项目内模块导入
from src.core.utils.logger import logger
from src.ui.common.style_sheet import WidgetStyleSheet
from src.ui.components.code_editor.controls import LineNumberArea
from src.ui.components.code_editor.highlight import JsonHighlighter


class CodeEditorBase(PlainTextEdit):
    """代码编辑器基类

    功能：
        - 显示行号
        - 当前行与多行选中高亮
    仅作为代码编辑器的基础类，不包含任何智能编辑功能
    """

    INDENT_WIDTH: int = 4  # 缩进宽度

    def __init__(self, parent: QWidget | None = None) -> None:
        """初始化代码编辑器

        Args:
            parent (QWidget | None, optional): 父控件。默认为 None。
        """
        super().__init__(parent)

        # 变量
        self.font_size: int = 12
        self.current_line: int = -1
        self.selection_start_line: int = -1
        self.selected_lines: set[int] = set()

        # 行号区域控件
        self.line_number_area = LineNumberArea(self)

        # 编辑器基础设置
        self.setReadOnly(False)
        self._setup_font()
        self._update_line_number_area_width()

        # 应用样式
        WidgetStyleSheet.CODE_EDITOR.apply(self)

        # 连接信号
        self._connect_signals()

    def _connect_signals(self) -> None:
        """连接所有信号与槽函数"""
        self.blockCountChanged.connect(self._on_update_line_number_area_width)
        self.updateRequest.connect(self._on_update_line_number_area)
        self.cursorPositionChanged.connect(self._on_handle_cursor_position_changed)

    def _setup_font(self) -> None:
        """设置编辑器字体"""
        font = self.font()
        font.setPointSize(self.font_size)
        self.setFont(font)

    def set_font_size(self, size: int) -> None:
        """设置编辑器字体大小

        Args:
            size (int): 字体大小
        """
        self.font_size = size
        self._setup_font()
        self._update_line_number_area_width()

    def line_number_area_width(self) -> int:
        """计算行号区域宽度

        Returns:
            int: 行号区域的宽度
        """
        max_block = max(1, self.blockCount())
        digits = len(str(max_block))
        return 2 + self.fontMetrics().horizontalAdvance("9") * digits

    def line_number_area_paint_event(self, event: QPaintEvent) -> None:
        """绘制行号区域

        Args:
            event (QPaintEvent): 绘制事件
        """
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
                painter.setPen(Qt.GlobalColor.gray)
                painter.drawText(rect, Qt.AlignmentFlag.AlignRight, line_number)

            block = block.next()
            top = bottom
            bottom = top + self.blockBoundingRect(block).height()
            block_number += 1

    def resizeEvent(self, event: QResizeEvent) -> None:
        """重写 resize 事件，更新行号区域位置

        Args:
            event (QResizeEvent): 调整大小事件
        """
        super().resizeEvent(event)
        content_rect = self.contentsRect()
        margin = 8
        rect = QRect(
            content_rect.left() + margin,
            content_rect.top(),
            self.line_number_area_width(),
            content_rect.height(),
        )
        self.line_number_area.setGeometry(rect)

    def _update_line_number_area_width(self) -> None:
        """更新行号区域宽度"""
        self.setViewportMargins(self.line_number_area_width() + 10, 0, 0, 0)

    def _on_update_line_number_area_width(self, _: int) -> None:
        """槽函数：更新行号区域宽度

        Args:
            _ (int): 块数量变化（未使用）
        """
        self._update_line_number_area_width()

    def _on_update_line_number_area(self, rect: QRect, dy: int) -> None:
        """槽函数：滚动或更新行号区域

        Args:
            rect (QRect): 需要更新的矩形区域
            dy (int): 垂直滚动距离
        """
        if dy:
            self.line_number_area.scroll(0, dy)
        else:
            self.line_number_area.update(0, rect.y(), self.line_number_area.width(), rect.height())

        if rect.contains(self.viewport().rect()):
            self._update_line_number_area_width()

    def setPlainText(self, text: str) -> None:
        """保留滚动位置设置文本

        Args:
            text (str): 要设置的文本内容
        """
        scroll_pos = self.verticalScrollBar().value()
        super().setPlainText(text)
        QApplication.processEvents()
        self.verticalScrollBar().setValue(scroll_pos)

    def mousePressEvent(self, event: QMouseEvent) -> None:
        """处理鼠标点击多行选择

        根据鼠标事件更新选择状态

        Args:
            event (QMouseEvent): 鼠标事件
        """
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
        self._highlight_selected_lines()
        self.line_number_area.update()

    def _on_handle_cursor_position_changed(self) -> None:
        """槽函数：光标移动处理多行选择与高亮"""
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

        self._highlight_selected_lines()
        self.line_number_area.update()
        self.viewport().update()

    def _highlight_selected_lines(self) -> None:
        """高亮当前选中行或多行"""
        extra_selections = []

        for line_number in self.selected_lines:
            selection = QTextEdit.ExtraSelection()
            selection.format.setBackground(QColor(255, 255, 255, 25))
            selection.format.setProperty(QTextFormat.Property.FullWidthSelection, True)

            cursor = QTextCursor(self.document().findBlockByNumber(line_number))
            selection.cursor = cursor

            extra_selections.append(selection)

        self.setExtraSelections(extra_selections)


class CodeEditor(CodeEditorBase):
    """
    代码编辑器类，提供智能编辑功能

    功能：
        - 自动补全括号/引号
        - 智能缩进与回车
        - Ctrl+Enter 继承缩进
    """

    AUTO_COMPLETE_CHARS: dict[str, str] = {"{": "}", "[": "]", "(": ")", '"': '"', "'": "'"}
    INDENT_SIZE: int = 4

    def __init__(self, parent: Optional[PlainTextEdit] = None) -> None:
        """初始化代码编辑器"""
        super().__init__(parent)
        self._setup_ui()

    def _setup_ui(self) -> None:
        """设置编辑器UI属性"""
        self.setLineWrapMode(PlainTextEdit.LineWrapMode.NoWrap)
        self.setTabChangesFocus(False)

    def keyPressEvent(self, event: QKeyEvent) -> None:
        """处理按键事件"""
        cursor = self.textCursor()
        text = self.toPlainText()

        if event.key() == Qt.Key.Key_Backspace:
            if self._handle_backspace(cursor, text) or self._handle_indent_backspace(cursor, text):
                return
        elif event.key() == Qt.Key.Key_Tab:
            cursor.insertText(" " * self.INDENT_SIZE)
            self.setTextCursor(cursor)
            return
        elif event.key() in (Qt.Key.Key_Return, Qt.Key.Key_Enter):
            if event.modifiers() & Qt.KeyboardModifier.ControlModifier:
                if self._handle_ctrl_return(cursor, text):
                    return
            else:
                if self._handle_return(cursor, text):
                    return
        elif event.text() in self.AUTO_COMPLETE_CHARS:
            self._handle_auto_complete(cursor, event.text())
            return
        elif self._should_skip_closing_character(cursor, event.text(), text):
            cursor.movePosition(QTextCursor.MoveOperation.Right)
            self.setTextCursor(cursor)
            return

        super().keyPressEvent(event)

    def _handle_backspace(self, cursor: QTextCursor, text: str) -> bool:
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

    def _handle_indent_backspace(self, cursor: QTextCursor, text: str) -> bool:
        """退格删除缩进空格"""
        pos = cursor.position()
        if pos == 0:
            return False

        line_start = text.rfind("\n", 0, pos) + 1
        col = pos - line_start

        if col > 0:
            cursor.movePosition(
                QTextCursor.MoveOperation.Left, QTextCursor.MoveMode.KeepAnchor, min(self.INDENT_SIZE, col)
            )
            if cursor.selectedText().isspace():
                cursor.removeSelectedText()
                self.setTextCursor(cursor)
                return True
        return False

    def _handle_return(self, cursor: QTextCursor, text: str) -> bool:
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
                cursor.insertText("\n" + " " * (indent + self.INDENT_SIZE))
                cursor.insertText("\n" + " " * indent + next_char)
                cursor.movePosition(QTextCursor.MoveOperation.Up)
                cursor.movePosition(QTextCursor.MoveOperation.EndOfLine)
                cursor.endEditBlock()
                self.setTextCursor(cursor)
                return True

        cursor.insertText("\n" + " " * indent)
        cursor.endEditBlock()
        self.setTextCursor(cursor)
        return True

    def _handle_ctrl_return(self, cursor: QTextCursor, text: str) -> bool:
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

    def _handle_auto_complete(self, cursor: QTextCursor, char: str) -> None:
        """处理自动补全括号或引号"""
        close_char = self.AUTO_COMPLETE_CHARS[char]
        cursor.beginEditBlock()
        cursor.insertText(char + close_char)
        cursor.movePosition(QTextCursor.MoveOperation.Left)
        cursor.endEditBlock()
        self.setTextCursor(cursor)

    def _should_skip_closing_character(self, cursor: QTextCursor, input_char: str, text: str) -> bool:
        """检查光标后是否已有闭合字符，需要跳过"""
        pos = cursor.position()
        return pos < len(text) and input_char in self.AUTO_COMPLETE_CHARS.values() and text[pos] == input_char


class JsonEditor(CodeEditor):
    """JSON 专用编辑器，带语法高亮和层级辅助线"""

    # 信号定义
    json_validated_signal = Signal(bool)
    json_error_signal = Signal(str, int)  # 错误信息, 行号

    def __init__(self, parent: Optional[QWidget] = None) -> None:
        """初始化 JSON 编辑器

        Args:
            parent: 父组件，可选
        """
        super().__init__(parent)

        # 实例变量
        self.error_line: int = -1
        self.json_highlighter: JsonHighlighter = JsonHighlighter(self.document())

        # 初始化
        self._setup_ui()
        self._connect_signals()

    def _setup_ui(self) -> None:
        """初始化界面设置"""
        # JSON 编辑器特有的 UI 设置
        self.setLineWrapMode(QPlainTextEdit.LineWrapMode.NoWrap)
        self.setTabStopDistance(40)  # 设置制表符宽度

    def _connect_signals(self) -> None:
        """连接信号与槽"""
        super()._connect_signals()
        self.cursorPositionChanged.connect(self._on_cursor_position_changed)
        self.updateRequest.connect(self._on_update_request)

    def set_json(self, json_data: str) -> None:
        """设置 JSON 数据并进行格式化

        Args:
            json_data: JSON 格式的字符串数据
        """
        try:
            parsed: dict | list = json.loads(json_data)
            pretty_json: str = json.dumps(parsed, indent=4, ensure_ascii=False)
            self.setPlainText(pretty_json)
            self.json_validated_signal.emit(True)
        except json.JSONDecodeError as e:
            logger.error(f"无效的 JSON 数据: {e}")
            self.setPlainText(json_data)
            self.json_error_signal.emit(str(e), -1)

    def get_json(self, compressed: bool = True) -> str:
        """获取当前 JSON 数据

        Args:
            compressed: 是否压缩为一行字符串，默认为 True

        Returns:
            JSON 字符串
        """
        try:
            parsed: dict | list = json.loads(self.toPlainText())
            if compressed:
                return json.dumps(parsed, ensure_ascii=False)
            else:
                return json.dumps(parsed, indent=4, ensure_ascii=False)
        except json.JSONDecodeError as e:
            logger.error(f"无效的 JSON 数据: {e}")
            return self.toPlainText()

    def check_json(self, show_tips: bool = True) -> bool:
        """检查当前文本是否为有效 JSON 并标记错误行

        Args:
            show_tips: 是否显示提示信息，默认为 True

        Returns:
            JSON 是否有效
        """
        self.error_line = -1  # 清除旧错误

        try:
            json_data: dict | list = json.loads(self.toPlainText())
            self.setExtraSelections([])

            if show_tips:
                self._show_success_tip()

            # 重新格式化 JSON
            formatted_json: str = json.dumps(json_data, indent=4, ensure_ascii=False)
            self.setPlainText(formatted_json)
            self.json_validated_signal.emit(True)

            return True

        except json.JSONDecodeError as e:
            self.error_line = e.lineno - 1
            self._highlight_error_line(self.error_line)
            logger.error(f"JSON 语法错误: {e}")

            if show_tips:
                self._show_error_tip(e)

            self.json_error_signal.emit(str(e), self.error_line)
            return False

    def _on_cursor_position_changed(self) -> None:
        """光标位置改变时的槽函数"""
        self.viewport().update()

    def _on_update_request(self, rect: QRect, dy: int) -> None:
        """更新请求的槽函数

        Args:
            rect: 更新区域
            dy: 垂直偏移量
        """
        self.viewport().update()

    def _highlight_error_line(self, line_number: int) -> None:
        """高亮并跳转到指定错误行

        Args:
            line_number: 错误行号
        """
        block = self.document().findBlockByNumber(line_number)
        if not block.isValid():
            return

        cursor = self.textCursor()
        cursor.setPosition(block.position())
        self.setTextCursor(cursor)
        self.centerCursor()

        extra_selection = QTextEdit.ExtraSelection()
        selection_format = extra_selection.format
        selection_format.setBackground(QColor(255, 0, 0, 50))
        selection_format.setProperty(QTextFormat.FullWidthSelection, True)
        extra_selection.cursor = cursor

        self.setExtraSelections([extra_selection])

    def _show_success_tip(self) -> None:
        """显示 JSON 校验成功的提示"""
        TeachingTip.create(
            target=self.parent(),
            title="JSON 语法正确!",
            content="校验完成, JSON 语法正确！",
            icon=FluentIcon.ACCEPT,
            duration=2000,
            parent=self,
        )

    def _show_error_tip(self, error: json.JSONDecodeError) -> None:
        """显示 JSON 校验失败的提示

        Args:
            error: JSON 解码错误对象
        """
        TeachingTip.create(
            target=self.parent(),
            title="JSON 语法错误!",
            content=f"校验完成, JSON 语法错误! 请检查高亮行上下几行\n {error}",
            icon=FluentIcon.CLOSE,
            duration=-1,
            parent=self,
        )

    def paintEvent(self, event: QPaintEvent | None) -> None:
        """绘制文本及多层级缩进辅助线

        Args:
            event: 绘制事件
        """
        super().paintEvent(event)
        self._draw_indentation_guides()

    def _draw_indentation_guides(self) -> None:
        """绘制缩进辅助线"""
        painter = QPainter(self.viewport())
        char_width: int = self.fontMetrics().horizontalAdvance(" ")
        normal_pen: QPen = QPen(QColor(200, 200, 200, 26))
        normal_pen.setWidth(1)
        highlight_pen: QPen = QPen(QColor(200, 200, 200, 128))
        highlight_pen.setWidth(2)

        viewport_bottom: int = self.viewport().height()

        # 分析 JSON 结构并记录层级范围
        level_ranges: dict[int, list[tuple[int, int]]] = self._analyze_json_structure()

        # 收集可见行信息
        visible_blocks: list[tuple[int, float, float]] = self._get_visible_blocks(viewport_bottom)

        # 绘制普通层级线
        self._draw_normal_level_lines(painter, level_ranges, visible_blocks, char_width, normal_pen)

        # 绘制光标所在层级高亮线
        self._draw_highlighted_level_lines(painter, level_ranges, visible_blocks, char_width, highlight_pen)

    def _analyze_json_structure(self) -> dict[int, list[tuple[int, int]]]:
        """分析 JSON 结构并返回层级范围信息

        Returns:
            层级范围字典，键为层级，值为范围列表
        """
        level_ranges: dict[int, list[tuple[int, int]]] = {}
        stack: list[tuple[int, int]] = []

        block_iter = self.document().firstBlock()
        line_number: int = 0

        while block_iter.isValid():
            line_text: str = block_iter.text()
            for char in line_text:
                if char in "{[":
                    stack.append((len(stack), line_number))
                elif char in "}]":
                    if stack:
                        level: int
                        start: int
                        level, start = stack.pop()
                        end_line: int = max(line_number - 1, start)
                        level_ranges.setdefault(level, []).append((start, end_line))
            block_iter = block_iter.next()
            line_number += 1

        return level_ranges

    def _get_visible_blocks(self, viewport_bottom: int) -> list[tuple[int, float, float]]:
        """获取当前可见的文本块信息

        Args:
            viewport_bottom: 视口底部位置

        Returns:
            可见块列表，每个元素为 (行号, 顶部位置, 底部位置)
        """
        visible_blocks: list[tuple[int, float, float]] = []
        block = self.firstVisibleBlock()
        top: float = self.blockBoundingGeometry(block).translated(self.contentOffset()).top()
        bottom: float = top + self.blockBoundingRect(block).height()

        while block.isValid() and top <= viewport_bottom:
            visible_blocks.append((block.blockNumber(), top, bottom))
            block = block.next()
            top = bottom
            bottom = top + self.blockBoundingRect(block).height()

        return visible_blocks

    def _draw_normal_level_lines(
        self,
        painter: QPainter,
        level_ranges: dict[int, list[tuple[int, int]]],
        visible_blocks: list[tuple[int, float, float]],
        char_width: int,
        pen: QPen,
    ) -> None:
        """绘制普通层级的缩进辅助线

        Args:
            painter: 绘制器
            level_ranges: 层级范围字典
            visible_blocks: 可见块列表
            char_width: 字符宽度
            pen: 画笔
        """
        for level, ranges in level_ranges.items():
            x: int = level * self.INDENT_SIZE * char_width + 4
            for start, end in ranges:
                for line_number, top, bottom in visible_blocks:
                    if start + 1 <= line_number <= end:
                        painter.setPen(pen)
                        painter.drawLine(x, top, x, bottom)

    def _draw_highlighted_level_lines(
        self,
        painter: QPainter,
        level_ranges: dict[int, list[tuple[int, int]]],
        visible_blocks: list[tuple[int, float, float]],
        char_width: int,
        pen: QPen,
    ) -> None:
        """绘制高亮显示的缩进辅助线（光标所在层级）

        Args:
            painter: 绘制器
            level_ranges: 层级范围字典
            visible_blocks: 可见块列表
            char_width: 字符宽度
            pen: 画笔
        """
        cursor_line: int = self.textCursor().block().blockNumber()
        for level, ranges in level_ranges.items():
            for start, end in ranges:
                if start < cursor_line <= end:
                    x: int = level * self.INDENT_SIZE * char_width + 4
                    for line_number, top, bottom in visible_blocks:
                        if start + 1 <= line_number <= end:
                            painter.setPen(pen)
                            painter.drawLine(x, top, x, bottom)
