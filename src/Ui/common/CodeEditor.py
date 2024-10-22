# -*- coding: utf-8 -*-

# 第三方库导入
from qfluentwidgets import PlainTextEdit, SmoothScrollDelegate, setFont
from qfluentwidgets.components.widgets.menu import TextEditMenu
from PySide6.QtGui import (
    QColor,
    QPainter,
    QMouseEvent,
    QPaintEvent,
    QFontDatabase,
    QTextCharFormat,
    QDesktopServices,
    QSyntaxHighlighter,
)
from PySide6.QtCore import Qt, QUrl, Slot, QRect, QSize, QRectF, QTimer, QRegularExpression
from PySide6.QtWidgets import QWidget, QTextBrowser


class CodeEditor(PlainTextEdit):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.line_number_area = LineNumberArea(self)

        # 连接信号和槽函数
        self.blockCountChanged.connect(self.update_line_number_area_width)
        self.updateRequest.connect(self.update_line_number_area)

        # 初始设置
        self.setReadOnly(True)
        self.update_line_number_area_width(0)
        self.set_monospace_font()

    def set_monospace_font(self) -> None:
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
        # 默认显示5位数空间
        digits, max_num = 4, max(1, self.blockCount())
        while max_num >= 10000:
            # 当行数达到五位数时调整显示空间
            max_num *= 0.1
            digits += 1
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
                number = str(block_number + 1)
                painter.drawText(
                    QRectF(content_left, top, self.line_number_area.width() - 12, self.fontMetrics().height()),
                    Qt.AlignmentFlag.AlignRight,
                    number,
                )

            block = block.next()
            top = bottom
            bottom = top + self.blockBoundingRect(block).height()
            block_number += 1

    def resizeEvent(self, event) -> None:
        super().resizeEvent(event)
        # 调整行号区域的位置和大小
        cr = self.contentsRect()
        width = self.lineNumberAreaWidth()
        rect = QRect(cr.left(), cr.top(), width, cr.height())
        self.line_number_area.setGeometry(rect)

    def setPlainText(self, text):
        # 保存当前的滚动位置
        scroll_position = self.verticalScrollBar().value()
        # 调用父类的 setText 方法更新文本
        super().setPlainText(text)

        # 使用 QTimer 延迟恢复滚动条位置
        QTimer.singleShot(0, lambda: self.verticalScrollBar().setValue(scroll_position))

    @Slot(int)
    def update_line_number_area_width(self, new_block_count: int) -> None:
        # 更新行号区域的宽度
        self.setViewportMargins(self.lineNumberAreaWidth() + 10, 0, 0, 0)

    @Slot(QRect, int)
    def update_line_number_area(self, rect: QRect, dy: int) -> None:
        # 更新行号区域的显示内容
        if dy:
            self.line_number_area.scroll(0, dy)
        else:
            width = self.line_number_area.width()
            self.line_number_area.update(0, rect.y(), width, rect.height())

        if rect.contains(self.viewport().rect()):
            self.update_line_number_area_width(0)


class UpdateLogEdit(QTextBrowser):
    """
    ## 更新日志页面使用的透明文本框
    """

    def __init__(self, parent=None):
        super().__init__(parent)
        self.scrollDelegate = SmoothScrollDelegate(self)
        self.setReadOnly(True)
        self.setOpenExternalLinks(True)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        setFont(self)

    def mousePressEvent(self, event: QMouseEvent) -> None:
        if event.button() == Qt.MouseButton.LeftButton and self.anchorAt(event.pos()):
            QDesktopServices.openUrl(QUrl(self.anchorAt(event.pos())))
        else:
            super().mousePressEvent(event)

    def contextMenuEvent(self, e):
        menu = TextEditMenu(self)
        menu.exec(e.globalPos(), ani=True)


class LineNumberArea(QWidget):
    def __init__(self, editor: CodeEditor):
        super().__init__(editor)
        self.code_editor = editor

    def sizeHint(self) -> QSize:
        # 返回推荐的尺寸大小
        return QSize(self.code_editor.lineNumberAreaWidth(), 0)

    def paintEvent(self, event: QPaintEvent) -> None:
        # 绘制事件处理，委托给CodeEditor中的lineNumberAreaPaintEvent方法
        self.code_editor.lineNumberAreaPaintEvent(event)


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
        test_format = QTextCharFormat()

        # 检查当前文本块是否匹配日志级别的模式
        match = self.pattern.match(text)

        # 如果没有匹配到，直接返回
        if not match or not match.hasMatch():
            return

        # 获取时间戳的起始位置和长度
        timestamp_start = match.capturedStart(1)
        timestamp_length = match.capturedLength(1)

        # 获取日志级别的起始位置和长度
        loglevel_start = match.capturedStart(2)
        loglevel_length = match.capturedLength(2)

        # 应用时间戳的格式
        test_format.setForeground(QColor(Qt.GlobalColor.lightGray))
        self.setFormat(timestamp_start, timestamp_length, test_format)

        # 检查捕获的日志级别是否在预定义列表中
        log_level = match.captured(2)
        if log_level in self.log_levels:
            # 设置对应日志级别的格式
            test_format.setForeground(self.formats[log_level].foreground().color())
            self.setFormat(loglevel_start, loglevel_length, test_format)


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
        test_format = QTextCharFormat()

        # 检查当前文本块是否匹配日志级别的模式
        match = self.pattern.match(text)

        # 如果没有匹配到，直接返回
        if not match or not match.hasMatch():
            return

        # 获取时间戳的起始位置和长度
        timestamp_start = match.capturedStart(1)
        timestamp_length = match.capturedLength(1)

        # 获取日志级别的起始位置和长度
        loglevel_start = match.capturedStart(2)
        loglevel_length = match.capturedLength(2)

        # 应用时间戳的格式
        test_format.setForeground(QColor(Qt.GlobalColor.darkGreen))
        self.setFormat(timestamp_start, timestamp_length, test_format)

        # 检查捕获的日志级别是否在预定义列表中
        log_level = match.captured(2)
        if log_level in self.log_levels:
            # 设置对应日志级别的格式
            test_format.setForeground(self.formats[log_level].foreground().color())
            self.setFormat(loglevel_start, loglevel_length, test_format)
