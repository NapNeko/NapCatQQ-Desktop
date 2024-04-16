# -*- coding: utf-8 -*-

"""
主页
"""

from abc import ABC
from loguru import logger
from typing import TYPE_CHECKING

from PySide6.QtCore import Qt, QRectF
from PySide6.QtGui import QPixmap, QPainter, QColor, QBrush, QPainterPath
from PySide6.QtWidgets import QWidget, QVBoxLayout, QLabel
from qfluentwidgets.components import ScrollArea
from creart import add_creator, exists_module, create
from creart.creator import AbstractCreator, CreateTargetInfo

from src.ui.StyleSheet import StyleSheet

if TYPE_CHECKING:
    from src.ui.main_window import MainWindow


class HomeWidget(ScrollArea):

    def __init__(self):
        super().__init__()

    def initialize(self, parent: "MainWindow") -> "HomeWidget":
        """
        初始化
        """
        # 创建显示控件和布局
        self.view = QWidget()
        self.view.setObjectName("view")
        self.vBoxLayout = QVBoxLayout(self.view)

        # 设置 ScrollArea
        self.setParent(parent)
        self.setObjectName("HomePage")
        self.setWidget(self.view)

        # 应用样式表
        StyleSheet.HOME_WIDGET.apply(self)

        return self


class HomeWidgetClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.ui.home_page.home", "HomeWidget"),)

    # 静态方法available()，用于检查模块"HomeWidget"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.ui.home_page.home")

    # 静态方法create()，用于创建HomeWidget类的实例，返回值为HomeWidget对象。
    @staticmethod
    def create(create_type: [HomeWidget]) -> HomeWidget:
        return HomeWidget()


add_creator(HomeWidgetClassCreator)
