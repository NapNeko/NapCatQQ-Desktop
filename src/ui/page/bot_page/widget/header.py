# -*- coding: utf-8 -*-

# 标准库导入
import math
from enum import Enum

# 第三方库导入
from qfluentwidgets import BreadcrumbBar, setFont
from PySide6.QtGui import QFont
from PySide6.QtWidgets import QVBoxLayout, QWidget


class HeaderWidget(QWidget):
    """页面顶部的信息标头"""

    class PageEnum(Enum):

        BOT_LIST = 0
        BOT_CONFIG = 1
        ADD_CONFIG = 2

    def __init__(self, parent: QWidget | None = None) -> None:
        """构造函数

        Args:
            parent (QWidget | None, optional): 父控件. Defaults to None.
        """
        super().__init__(parent)

        # 创建控件
        self.breadcrumb_bar = BreadcrumbBar(self)
        self.v_box_layout = QVBoxLayout(self)

        # 设置控件
        setFont(self.breadcrumb_bar, 28, QFont.Weight.Bold)
        self.breadcrumb_bar.addItem("title", self.tr("Bot"))
        self.breadcrumb_bar.setSpacing(16)
        self.breadcrumb_bar.setEnabled(False)
        self.setFixedHeight(48)

        # 设置布局
        self.v_box_layout.setContentsMargins(0, 0, 0, 0)
        self.v_box_layout.setSpacing(6)
        self.v_box_layout.addWidget(self.breadcrumb_bar)

    def setup_breadcrumb_bar(self, index: int):
        """通过index设置对应的breadcrumb_bar"""
        self.breadcrumb_bar.clear()

        match self.PageEnum(index):
            case self.PageEnum.BOT_LIST:
                self.breadcrumb_bar.addItem("title", self.tr("Bot"))
                self.breadcrumb_bar.addItem("bot_list", self.tr("List"))
            case self.PageEnum.BOT_CONFIG:
                self.breadcrumb_bar.addItem("title", self.tr("Bot"))
                self.breadcrumb_bar.addItem("bot_list", self.tr("List"))
                self.breadcrumb_bar.addItem("bot_config", self.tr("Config"))
            case self.PageEnum.ADD_CONFIG:
                self.breadcrumb_bar.addItem("title", self.tr("Bot"))
                self.breadcrumb_bar.addItem("bot_list", self.tr("List"))
                self.breadcrumb_bar.addItem("bot_config", self.tr("Add"))
