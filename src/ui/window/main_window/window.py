# -*- coding: utf-8 -*-
# 标准库导入
from typing import Optional

# 第三方库导入
from qfluentwidgets import Theme, FluentIcon, SplashScreen, MSFluentWindow, NavigationItemPosition
from PySide6.QtGui import QIcon
from PySide6.QtCore import QSize
from PySide6.QtWidgets import QApplication

# 项目内模块导入
from src.ui.common.icon import NapCatDesktopIcon
from src.ui.page.add_page import AddWidget
from src.core.config import cfg
from src.ui.page.HomePage import HomeWidget
from src.ui.page.unit_page import UnitWidget
from src.ui.page.setup_page import SetupWidget
from src.ui.page.bot_list_page import BotListWidget
from src.core.utils.singleton import singleton
from src.ui.window.MainWindow.TitleBar import CustomTitleBar
from src.ui.window.MainWindow.SystemTryIcon import SystemTrayIcon


@singleton
class MainWindow(MSFluentWindow):
    """
    ## 程序的主窗体
    """

    trayIcon: Optional[SystemTrayIcon]
    title_bar: Optional[CustomTitleBar]
    splashScreen: Optional[SplashScreen]

    def __init__(self) -> None:
        """构造函数"""
        super().__init__()

    def initialize(self) -> None:
        """初始化"""
        # 调用方法
        self.setWindow()
        self.setItem()
        self.setTrayIcon()
        # 组件加载完成结束 SplashScreen
        self.splashScreen.finish()

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

    def setItem(self) -> None:
        """
        设置侧边栏
        """

        # 添加子页面
        self.addSubInterface(
            interface=HomeWidget().initialize(self),
            icon=FluentIcon.HOME,
            text=self.tr("主页"),
            position=NavigationItemPosition.TOP,
        )

        self.addSubInterface(
            interface=AddWidget().initialize(self),
            icon=FluentIcon.ADD_TO,
            text=self.tr("添加"),
            position=NavigationItemPosition.TOP,
        )
        self.addSubInterface(
            interface=BotListWidget().initialize(self),
            icon=FluentIcon.MENU,
            text=self.tr("列表"),
            position=NavigationItemPosition.TOP,
        )
        self.addSubInterface(
            interface=UnitWidget().initialize(self),
            icon=FluentIcon.EMOJI_TAB_SYMBOLS,
            text=self.tr("组件"),
            position=NavigationItemPosition.BOTTOM,
        )
        self.addSubInterface(
            interface=SetupWidget().initialize(self),
            icon=FluentIcon.SETTING,
            text=self.tr("设置"),
            position=NavigationItemPosition.BOTTOM,
        )

    def setTrayIcon(self):
        """
        ## 设置托盘图标
        """
        self.trayIcon = SystemTrayIcon(self)
        self.trayIcon.show()
