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

        self.unit_widget: Optional[UnitWidget] = None
        self.splashScreen: Optional[SplashScreen] = None
        self.setup_widget: Optional[SetupWidget] = None
        self.add_widget: Optional[AddWidget] = None
        self.bot_list_widget: Optional[BotListWidget] = None
        self.home_widget: Optional[HomeWidget] = None

        self.fix_widget_button: Optional[NavigationBarPushButton] = None
        self.unit_widget_button: Optional[NavigationBarPushButton] = None
        self.home_widget_button: Optional[NavigationBarPushButton] = None
        self.add_widget_button: Optional[NavigationBarPushButton] = None
        self.bot_list_widget_button: Optional[NavigationBarPushButton] = None
        self.setup_widget_button: Optional[NavigationBarPushButton] = None

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
        self.setMinimumSize(1024, 720)
        desktop = QApplication.screens()[0].availableGeometry()
        width, height = desktop.width(), desktop.height()
        self.move(width // 2 - self.width() // 2, height // 2 - self.height() // 2)
        # 调整窗体透明度
        self.setWindowOpacity(cfg.get(cfg.windowOpacity) / 100)
        # 创建 Splash Screen
        self.splashScreen = SplashScreen(":Global/logo.png", self, True)
        self.splashScreen.setIconSize(QSize(128, 128))
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
        self.setup_widget = it(SetupWidget).initialize(self)
        self.add_widget = it(AddWidget).initialize(self)
        self.bot_list_widget = it(BotListWidget).initialize(self)
        self.unit_widget = it(UnitWidget).initialize(self)
        self.home_widget = it(HomeWidget).initialize(self)

        # 添加子页面
        self.home_widget_button = self.addSubInterface(
            interface=self.home_widget,
            icon=FluentIcon.HOME,
            text=self.tr("主页"),
            position=NavigationItemPosition.TOP,
        )

        self.add_widget_button = self.addSubInterface(
            interface=self.add_widget,
            icon=FluentIcon.ADD_TO,
            text=self.tr("添加"),
            position=NavigationItemPosition.TOP,
        )
        self.bot_list_widget_button = self.addSubInterface(
            interface=self.bot_list_widget,
            icon=FluentIcon.MENU,
            text=self.tr("列表"),
            position=NavigationItemPosition.TOP,
        )
        self.unit_widget_button = self.addSubInterface(
            interface=self.unit_widget,
            icon=FluentIcon.EMOJI_TAB_SYMBOLS,
            text=self.tr("组件"),
            position=NavigationItemPosition.BOTTOM,
        )
        self.setup_widget_button = self.addSubInterface(
            interface=self.setup_widget,
            icon=FluentIcon.SETTING,
            text=self.tr("设置"),
            position=NavigationItemPosition.BOTTOM,
        )

        logger.success("侧边栏构建完成")

    def setPage(self) -> None:
        """
        ## 窗口创建完成进行一些处理
        """
        self.bot_list_widget.botList.updateList()

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
