# -*- coding: utf-8 -*-

"""
构建主窗体
"""

from abc import ABC
from creart import it, add_creator, exists_module
from creart.creator import AbstractCreator, CreateTargetInfo
from loguru import logger

from PySide6.QtWidgets import QApplication, QWidget
from PySide6.QtCore import QSize
from PySide6.QtGui import QDesktopServices

from qfluentwidgets.window import MSFluentWindow, SplashScreen
from qfluentwidgets.common import FluentIcon
from qfluentwidgets.components import NavigationItemPosition

from src.ui.resource import resource
from src.ui.icon import MainWindowIcon
from src.ui.main_window.title_bar import CustomTitleBar
from src.ui.home_page import HomeWidget


class MainWindow(MSFluentWindow):

    def __init__(self) -> None:
        """
        程序的主窗体
        """
        super().__init__()
        self.set_window()
        self.set_item()

        # 组件加载完成结束 SplashScreen
        self.splashScreen.finish()
        logger.success("窗体构建完成")

    def set_window(self) -> None:
        """
        设置窗体
        """
        # 标题栏部分
        self.setTitleBar(CustomTitleBar(self))
        # 窗体大小以及设置打开时居中
        self.setMinimumSize(900, 580)
        desktop = QApplication.screens()[0].availableGeometry()
        width, height = desktop.width(), desktop.height()
        self.move(width // 2 - self.width() // 2, height // 2 - self.height() // 2)
        # 创建 Splash Screen
        self.splashScreen = SplashScreen(MainWindowIcon.LOGO, self, True)
        self.splashScreen.setIconSize(QSize(128, 128))
        self.splashScreen.raise_()
        # 显示窗体
        self.show()
        logger.success("窗体设置完成")

    def set_item(self) -> None:
        """
        设置侧边栏
        """
        self.test_widget1 = QWidget()
        self.test_widget1.setObjectName("1")
        self.test_widget2 = QWidget()
        self.test_widget2.setObjectName("2")
        self.test_widget3 = QWidget()
        self.test_widget3.setObjectName("3")
        self.test_widget4 = QWidget()
        self.test_widget4.setObjectName("4")
        # 添加子页面
        self.addSubInterface(
            interface=self.home_widget,
            icon=FluentIcon.HOME,
            text=self.tr("Home"),
            position=NavigationItemPosition.TOP
        )

        self.addSubInterface(
            interface=self.test_widget2,
            icon=FluentIcon.ADD_TO,
            text=self.tr("Add Bot"),
            position=NavigationItemPosition.TOP
        )
        self.addSubInterface(
            interface=self.test_widget3,
            icon=FluentIcon.MENU,
            text=self.tr("Bot List"),
            position=NavigationItemPosition.TOP
        )

        self.addSubInterface(
            interface=self.test_widget4,
            icon=FluentIcon.SETTING,
            text=self.tr("Setup"),
            position=NavigationItemPosition.BOTTOM
        )

        logger.success("侧边栏构建完成")


class MainWindowClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.ui.main_window.window", "MainWindow"),)

    # 静态方法available()，用于检查模块"MainWindow"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.ui.main_window.window")

    # 静态方法create()，用于创建MainWindow类的实例，返回值为MainWindow对象。
    @staticmethod
    def create(create_type: [MainWindow]) -> MainWindow:
        return MainWindow()


add_creator(MainWindowClassCreator)
