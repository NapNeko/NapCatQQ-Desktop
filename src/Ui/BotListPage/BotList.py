# -*- coding: utf-8 -*-

"""
机器人列表
"""
from abc import ABC
from typing import TYPE_CHECKING, Self

from creart import add_creator, exists_module
from creart.creator import AbstractCreator, CreateTargetInfo
from qfluentwidgets import ScrollArea
from src.Ui.StyleSheet import StyleSheet

if TYPE_CHECKING:
    from src.Ui.MainWindow import MainWindow


class BotListWidget(ScrollArea):

    def __init__(self):
        super().__init__()

    def initialize(self, parent: "MainWindow") -> Self:
        """
        初始化
        """

        # 设置 ScrollArea
        self.setParent(parent),
        self.setObjectName("BotListPage")
        self.setWidgetResizable(True)

        # 应用样式表
        StyleSheet.BOT_LIST_WIDGET.apply(self)

        return self


class BotListWidgetClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.Ui.BotListPage.BotList", "BotListWidget"),)

    # 静态方法available()，用于检查模块"BotList"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.Ui.BotListPage.BotList")

    # 静态方法create()，用于创建BotListWidget类的实例，返回值为BotListWidget对象。
    @staticmethod
    def create(create_type: [BotListWidget]) -> BotListWidget:
        return BotListWidget()


add_creator(BotListWidgetClassCreator)
