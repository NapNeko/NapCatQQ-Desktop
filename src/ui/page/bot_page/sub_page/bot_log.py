# -*- coding: utf-8 -*-
"""此模块包含了运行日志展示"""
# 标准库导入
from creart import it

# 第三方库导入
from qfluentwidgets import FluentIcon, HeaderCardWidget, TransparentPushButton, TransparentToolButton, setFont
from PySide6.QtCore import Qt
from PySide6.QtGui import QTextCursor
from PySide6.QtWidgets import QVBoxLayout, QWidget, QPlainTextEdit

# 项目内模块导入
from src.core.config import Config
from src.core.utils.run_napcat import ManagerNapCatQQLog, NapCatQQProcessLog
from src.ui.components.code_editor.exhibit import CodeExibit


class BotLogPage(QWidget):
    """Bot 日志子页面"""

    def __init__(self, parent: QWidget | None = None) -> None:
        """构造函数"""
        super().__init__(parent)
        # 创建属性
        self._config: Config | None = None

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
    def set_current_log_manager(self, config: Config) -> None:
        """设置当前展示的 Bot Log"""

        # 拿到 log 实例并判断是否为空
        if (log := it(ManagerNapCatQQLog).get_log(str(config.bot.QQID))) is None:
            self.log_view.setPlainText(self.tr("未找到对应的日志信息"))
            return

        # 设置控件
        self.view.setTitle(self.tr(f"Bot 日志({str(config.bot.QQID)})"))
        self.slot_set_log_view(log.get_log_content())

        # 调用方法
        self.set_current_config(config)

        # 连接信号
        log.output_log_signal.connect(self.slot_insert_log_view)

    def set_current_config(self, config: Config) -> None:
        """设置当前配置"""
        self._config = config

    # ==================== 槽函数 ====================
    def slot_return_button(self) -> None:
        """返回按钮槽函数"""
        # 项目内模块导入
        from src.ui.page.bot_page import BotPage

        # 返回列表页面
        page = it(BotPage)
        page.view.setCurrentWidget(page.bot_list_page)

        # 确保当前配置不是 None
        if self._config is None:
            return

        # 取消信号连接
        if (log := it(ManagerNapCatQQLog).get_log(str(self._config.bot.QQID))) is not None:
            log.output_log_signal.disconnect(self.slot_insert_log_view)

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
