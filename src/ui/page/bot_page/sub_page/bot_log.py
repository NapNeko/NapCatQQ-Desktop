# -*- coding: utf-8 -*-
"""此模块包含了运行日志展示"""
# 标准库导入
from tkinter import N

# 第三方库导入
from qfluentwidgets import FluentIcon, HeaderCardWidget, TransparentPushButton, TransparentToolButton, setFont
from PySide6.QtCore import Qt
from PySide6.QtGui import QTextCursor
from PySide6.QtWidgets import QVBoxLayout, QWidget, QPlainTextEdit

# 项目内模块导入
from src.core.utils.run_napcat import NapCatQQProcessLogger
from src.ui.components.code_editor.exhibit import CodeExibit


class BotLogPage(QWidget):
    """Bot 日志子页面"""

    def __init__(self, parent: QWidget | None = None) -> None:
        """构造函数"""
        super().__init__(parent)
        # 创建属性
        self._log_manager = None

        # 创建控件
        self.view = HeaderCardWidget(self)
        self.log_view = CodeExibit(self)
        self.font_enlarge_button = TransparentToolButton(FluentIcon.ADD, self.view)
        self.font_shrink_button = TransparentToolButton(FluentIcon.REMOVE, self.view)
        self.return_button = TransparentPushButton(FluentIcon.LEFT_ARROW, self.tr("返回"), self.view)

        # 设置控件
        self.view.setTitle(self.tr("Bot 日志"))
        self.log_view.set_font_size(10)
        self.log_view.setLineWrapMode(QPlainTextEdit.LineWrapMode.WidgetWidth)

        # 设置布局
        self.view.headerLayout.addStretch(1)
        self.view.headerLayout.addWidget(self.font_enlarge_button)
        self.view.headerLayout.addWidget(self.font_shrink_button)
        self.view.headerLayout.addWidget(self.return_button)
        self.view.viewLayout.setContentsMargins(2, 4, 2, 0)
        self.view.viewLayout.addWidget(self.log_view, 1)

        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 0, 0, 8)
        self.v_box_layout.addWidget(self.view)

        # 连接信号
        self.font_enlarge_button.clicked.connect(self.slot_font_enlarge_button)
        self.font_shrink_button.clicked.connect(self.slot_font_shrink_button)
        self.return_button.clicked.connect(self.slot_return_button)

    # ==================== 公共方法 ==================
    def set_current_log_manager(self, log_manager: NapCatQQProcessLogger) -> None:
        """设置当前展示的 Bot Log"""
        self._log_manager = log_manager
        # 设置控件
        self.view.setTitle(self.tr(f"Bot 日志({log_manager._qq_id})"))
        self.slot_set_log_view(log_manager.get_log())
        # 连接信号
        log_manager.handle_output_log_signal.connect(self.slot_insert_log_view)

    # ==================== 槽函数 ====================
    def slot_return_button(self) -> None:
        """返回按钮槽函数"""
        # 项目内模块导入
        from src.ui.page.bot_page import BotPage

        BotPage().view.setCurrentWidget(BotPage().bot_list_page)
        self._log_manager.handle_output_log_signal.disconnect()

    def slot_set_log_view(self, data: str) -> None:
        """设置当前 log_view 内容"""
        self.log_view.setPlainText(data)

    def slot_insert_log_view(self, data: str) -> None:
        """插入内容到 log_view"""
        cursor = self.log_view.textCursor()
        cursor.movePosition(QTextCursor.MoveOperation.End)
        cursor.insertText(data)
        self.log_view.setTextCursor(cursor)

    def slot_font_enlarge_button(self) -> None:
        """放大字体槽函数"""
        self.log_view.set_font_size(self.log_view.font_size + 1)

    def slot_font_shrink_button(self) -> None:
        """缩小字体槽函数"""
        self.log_view.set_font_size(self.log_view.font_size - 1)
