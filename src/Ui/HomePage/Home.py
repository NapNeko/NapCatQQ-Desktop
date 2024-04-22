# -*- coding: utf-8 -*-

"""
主页
"""

from abc import ABC
from typing import TYPE_CHECKING, Self

from PySide6.QtCore import Qt
from creart import add_creator, exists_module
from creart.creator import AbstractCreator, CreateTargetInfo
from qfluentwidgets.components import (
    InfoBar, InfoBarIcon, InfoBarPosition, PushButton
)

from src.Core.Config import StartOpenHomePageViewEnum as SE
from src.Core.Config import cfg
from src.Ui import PageBase
from src.Ui.HomePage.ContentView import ContentViewWidget
from src.Ui.HomePage.DisplayView import DisplayViewWidget
from src.Ui.StyleSheet import StyleSheet

if TYPE_CHECKING:
    from src.Ui.MainWindow import MainWindow


class HomeWidget(PageBase):

    def __init__(self):
        super().__init__()

    def initialize(self, parent: "MainWindow") -> Self:
        """
        初始化
        """
        # 创建显示控件
        self.view = self.__judgeView()

        # 设置 View 和 ScrollArea
        if isinstance(self.view, DisplayViewWidget):
            self.view.go_btn_signal.connect(self.__goBtnSlot)

        self.setParent(parent)
        self.setObjectName("HomePage")
        self.setWidgetResizable(True)
        self.setWidget(self.view)

        # 调用方法
        self.updateBgImage()

        # 应用样式表
        StyleSheet.HOME_WIDGET.apply(self)

        return self

    def __goBtnSlot(self):
        """
        Start Using 的槽函数
        """
        self.view.deleteLater()
        self.view = ContentViewWidget()
        self.takeWidget()
        self.setWidget(self.view)

        if cfg.get(cfg.HideUsGoBtnTips):
            # 是否隐藏提示
            return

        info = InfoBar(
            icon=InfoBarIcon.INFORMATION,
            title="Tips",
            content=self.tr(
                "You can choose the page to display at \n"
                "startup in the settings page"
            ),
            orient=Qt.Orientation.Vertical,
            isClosable=True,
            position=InfoBarPosition.BOTTOM_RIGHT,
            duration=10000,
            parent=self
        )
        info_button = PushButton(self.tr("Don't show again"))
        info_button.clicked.connect(
            lambda: cfg.set(cfg.HideUsGoBtnTips, True, True)
        )
        info_button.clicked.connect(info.close)
        info.addWidget(info_button)
        info.show()

    @staticmethod
    def __judgeView() -> DisplayViewWidget | ContentViewWidget:
        """
        判断并加载相应的 Widget。
        根据配置确定是打开首页视图还是内容视图。
        """
        start_page = cfg.get(cfg.StartOpenHomePageView)
        return DisplayViewWidget() if start_page == SE.DISPLAY_VIEW else ContentViewWidget()


class HomeWidgetClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.Ui.HomePage.Home", "HomeWidget"),)

    # 静态方法available()，用于检查模块"HomeWidget"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.Ui.HomePage.Home")

    # 静态方法create()，用于创建HomeWidget类的实例，返回值为HomeWidget对象。
    @staticmethod
    def create(create_type: [HomeWidget]) -> HomeWidget:
        return HomeWidget()


add_creator(HomeWidgetClassCreator)
