# -*- coding: utf-8 -*-
import json
from typing import List, TYPE_CHECKING

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget, QVBoxLayout
from creart import it
from qfluentwidgets import ScrollArea, InfoBar, InfoBarPosition, FlowLayout

from src.Core.Config.ConfigModel import Config
from src.Core.PathFunc import PathFunc
from src.Ui.BotListPage.BotCard import BotCard

if TYPE_CHECKING:
    from src.Ui.BotListPage import BotListWidget


class BotList(ScrollArea):
    """
    ## BotListWidget 内部的机器人列表

    自动读取配置文件中已有的的机器人配置
    """

    def __init__(self, parent: "BotListWidget"):
        """
        ## 初始化
        """
        super().__init__(parent=parent)
        self._createView()
        self.updateList()
        self._initWidget()

    def _initWidget(self):
        """
        ## 设置 ScrollArea
        """
        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

    def _createView(self):
        """
        ## 构建并设置 ScrollArea 所需的 widget
        """
        self.view = QWidget(self)
        self.cardLayout = FlowLayout(self.view)
        self.cardLayout.setContentsMargins(0, 0, 0, 0)
        self.cardLayout.setSpacing(4)
        self.view.setObjectName("BotListView")
        self.view.setLayout(self.cardLayout)

    def updateList(self):
        """
        ## 更新机器人列表
        """
        self._parseList()
        # 卸载掉原有的 card
        if self.cardLayout.count() != 0:
            self.cardLayout.takeAllWidgets()

        # 重新添加到布局中
        for bot in self.bot_list:
            card = BotCard(bot)
            self.cardLayout.addWidget(card)

    def _parseList(self):
        """
        ## 解析机器人配置(如果有)
        """
        try:
            # 读取配置列表
            with open(str(it(PathFunc).bot_config_path), "r", encoding="utf-8") as f:
                bot_configs = json.load(f)
            self.bot_list: List[Config] = [Config(**config) for config in bot_configs]

        except FileNotFoundError:
            # 如果文件不存在则创建一个
            with open(str(it(PathFunc).bot_config_path), "w", encoding="utf-8") as f:
                json.dump([], f, indent=4)

        except ValueError as e:
            # 如果配置文件解析失败则提示错误信息
            self._showErrorBar(self.tr("Unable to load bot list"), str(e))

    def _showErrorBar(self, title: str, content: str):
        """
        ## 显示错误消息条
        """
        InfoBar.error(
            title=title,
            content=content,
            orient=Qt.Orientation.Vertical,
            duration=50000,
            position=InfoBarPosition.BOTTOM_RIGHT,
            parent=self.parent().parent(),  # 应该是 BotListWidget
        )
