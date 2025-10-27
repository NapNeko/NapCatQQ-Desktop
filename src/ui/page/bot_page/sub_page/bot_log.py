# -*- coding: utf-8 -*-
"""此模块包含了运行日志展示"""
from tkinter import N
from PySide6.QtWidgets import QWidget, QVBoxLayout
from PySide6.QtCore import Qt, Signal
from PySide6.QtGui import QTextCursor

from qfluentwidgets import HeaderCardWidget, TransparentPushButton, FluentIcon

from src.ui.components.code_editor.exhibit import CodeExibit

from src.core.utils.run_napcat import NapCatQQProcessLogger


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
        self.return_button = TransparentPushButton(FluentIcon.LEFT_ARROW, self.tr("返回"), self.view)

        # 设置控件
        self.view.setTitle(self.tr("Bot 日志"))

        # 设置布局
        self.view.headerLayout.addWidget(self.return_button, alignment=Qt.AlignmentFlag.AlignRight)
        self.view.viewLayout.setContentsMargins(2, 4, 2, 0)
        self.view.viewLayout.addWidget(self.log_view, 1)

        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 0, 0, 8)
        self.v_box_layout.addWidget(self.view)

        # 连接信号
        self.return_button.clicked.connect(self.slot_return_button)

    # ==================== 公共方法 ==================
    def set_current_log_manager(self, log_manager: NapCatQQProcessLogger) -> None:
        """设置当前展示的 Bot Log"""
        self._log_manager = log_manager
        self.slot_set_log_view(log_manager.get_log())
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
