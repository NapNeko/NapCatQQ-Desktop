# -*- coding: utf-8 -*-
"""
此模块用于展示软件内 Bot 页面, 这是软件的核心页面, 包含很多子页面
"""
from __future__ import annotations

# 标准库导入
from typing import TYPE_CHECKING

from PySide6.QtWidgets import QVBoxLayout, QWidget

# 项目内模块导入
from src.core.utils.singleton import singleton
from src.ui.components.stacked_widget import TransparentStackedWidget

from .sub_page import BotListPage
from .widget import HeaderWidget

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.window.main_window import MainWindow


@singleton
class BotPage(QWidget):
    """Bot 页面"""

    def __init__(self, parent=None) -> None:
        super().__init__(parent)

        # 创建控件
        self.header = HeaderWidget(self)
        self.view = TransparentStackedWidget(self)
        self.vBoxLayout = QVBoxLayout(self)

        # 设置属性
        self.setObjectName("bot_page")

        # 设置布局
        self.vBoxLayout.setContentsMargins(24, 20, 24, 10)
        self.vBoxLayout.setSpacing(16)

        self.vBoxLayout.addWidget(self.header, 1)
        self.vBoxLayout.addWidget(self.view, 9)

        # 调用方法
        self.setup_view()

    def setup_view(self) -> None:
        """设置 view 界面"""
        # 创建 sub page
        self.bot_list_page = BotListPage(self)

        # 添加到 view
        self.view.addWidget(self.bot_list_page)

        # 设置初始页面
        self.view.setCurrentWidget(self.bot_list_page)
