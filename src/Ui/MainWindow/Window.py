# -*- coding: utf-8 -*-
# 标准库导入
from typing import Optional

# 第三方库导入
from qfluentwidgets import Theme, SplashScreen, MSFluentWindow
from PySide6.QtGui import QIcon
from PySide6.QtCore import QSize
from PySide6.QtWidgets import QApplication

# 项目内模块导入
from src.ui.Icon import NapCatDesktopIcon
from src.ui.resource import resource
from src.core.utils.path import PathFunc
from src.core.utils.singleton import singleton
from src.ui.MainWindow.SystemTryIcon import SystemTrayIcon


@singleton
class MainWindow(MSFluentWindow):
    """
    ## 程序的主窗体
    """

    trayIcon: Optional[SystemTrayIcon]
    splashScreen: Optional[SplashScreen]

    def __init__(self) -> None:
        """构造函数"""
        super().__init__()

    def initialize(self) -> None:
        """初始化"""
        # 执行路径验证
        PathFunc().path_validator()
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
        self.setWindowIcon(QIcon(NapCatDesktopIcon.LOGO.path(Theme.LIGHT)))
        # 窗体大小以及设置打开时居中
        self.setMinimumSize(1148, 720)
        desktop = QApplication.screens()[0].availableGeometry()
        width, height = desktop.width(), desktop.height()
        self.move(width // 2 - self.width() // 2, height // 2 - self.height() // 2)
        # 创建 Splash Screen
        self.splashScreen = SplashScreen(":Global/image/Global/logo.png", self, True)
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

    def setTrayIcon(self):
        """
        ## 设置托盘图标
        """
        self.trayIcon = SystemTrayIcon(self)
        self.trayIcon.show()

    def closeEvent(self, event):
        self.hide()
        event.ignore()
