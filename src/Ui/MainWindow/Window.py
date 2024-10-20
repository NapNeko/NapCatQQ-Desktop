# -*- coding: utf-8 -*-

"""
构建主窗体
"""
# 标准库导入
from abc import ABC
from typing import Optional

# 第三方库导入
from creart import it, add_creator, exists_module
from loguru import logger
from creart.creator import AbstractCreator, CreateTargetInfo
from qfluentwidgets import (
    Theme,
    FluentIcon,
    SplashScreen,
    MSFluentWindow,
    NavigationItemPosition,
    NavigationBarPushButton,
)
from PySide6.QtGui import QIcon
from PySide6.QtCore import QSize
from PySide6.QtWidgets import QApplication

# 项目内模块导入
from src.Ui.Icon import NapCatDesktopIcon
from src.Ui.AddPage import AddWidget
from src.Core.Config import cfg
from src.Ui.HomePage import HomeWidget
from src.Ui.resource import resource
from src.Ui.UnitPage import UnitWidget
from src.Ui.SetupPage import SetupWidget
from src.Ui.BotListPage import BotListWidget
from src.Ui.MainWindow.TitleBar import CustomTitleBar
from src.Ui.MainWindow.SystemTryIcon import SystemTrayIcon


class MainWindow(MSFluentWindow):
    """
    ## 程序的主窗体
    """

    def __init__(self) -> None:
        super().__init__()
        self.trayIcon: Optional[SystemTrayIcon] = None

    def initialize(self) -> None:
        """
        ## 初始化程序, 并显示窗体
        """
        self.setWindow()
        self.setItem()
        self.setPage()
        self.setTrayIcon()
        # 组件加载完成结束 SplashScreen
        self.splashScreen.finish()
        logger.success("窗体构建完成")

    def setWindow(self) -> None:
        """
        设置窗体
        """
        # 标题栏部分
        self.title_bar = CustomTitleBar(self)
        self.setTitleBar(self.title_bar)
        self.setWindowIcon(QIcon(NapCatDesktopIcon.LOGO.path(Theme.LIGHT)))
        # 窗体大小以及设置打开时居中
        self.setMinimumSize(1148, 720)
        desktop = QApplication.screens()[0].availableGeometry()
        width, height = desktop.width(), desktop.height()
        self.move(width // 2 - self.width() // 2, height // 2 - self.height() // 2)
        # 调整窗体透明度
        self.setWindowOpacity(cfg.get(cfg.windowOpacity) / 100)
        # 创建 Splash Screen
        self.splashScreen = SplashScreen(":Global/image/Global/napcat.png", self, True)
        self.splashScreen.setIconSize(QSize(360, 260))
        self.splashScreen.raise_()
        # 显示窗体
        self.show()
        # 挂起
        QApplication.processEvents()
        logger.success("窗体设置完成")

    def setItem(self) -> None:
        """
        设置侧边栏
        """

        # 添加子页面
        self.addSubInterface(
            interface=it(HomeWidget).initialize(self),
            icon=FluentIcon.HOME,
            text=self.tr("主页"),
            position=NavigationItemPosition.TOP,
        )

        self.addSubInterface(
            interface=it(AddWidget).initialize(self),
            icon=FluentIcon.ADD_TO,
            text=self.tr("添加"),
            position=NavigationItemPosition.TOP,
        )
        self.addSubInterface(
            interface=it(BotListWidget).initialize(self),
            icon=FluentIcon.MENU,
            text=self.tr("列表"),
            position=NavigationItemPosition.TOP,
        )
        self.addSubInterface(
            interface=it(UnitWidget).initialize(self),
            icon=FluentIcon.EMOJI_TAB_SYMBOLS,
            text=self.tr("组件"),
            position=NavigationItemPosition.BOTTOM,
        )
        self.addSubInterface(
            interface=it(SetupWidget).initialize(self),
            icon=FluentIcon.SETTING,
            text=self.tr("设置"),
            position=NavigationItemPosition.BOTTOM,
        )

        logger.success("侧边栏构建完成")

    @staticmethod
    def setPage() -> None:
        """
        ## 窗口创建完成进行一些处理
        """
        # 项目内模块导入
        from src.Ui.BotListPage import BotListWidget
        it(BotListWidget).botList.updateList()

    def setTrayIcon(self):
        """
        ## 设置托盘图标
        """
        self.trayIcon = SystemTrayIcon(self)
        self.trayIcon.show()

    def closeEvent(self, event):
        self.hide()
        event.ignore()


class MainWindowClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.Ui.MainWindow.Window", "MainWindow"),)

    # 静态方法available()，用于检查模块"MainWindow"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.Ui.MainWindow.Window")

    # 静态方法create()，用于创建MainWindow类的实例，返回值为MainWindow对象。
    @staticmethod
    def create(create_type: [MainWindow]) -> MainWindow:
        return MainWindow()


add_creator(MainWindowClassCreator)
