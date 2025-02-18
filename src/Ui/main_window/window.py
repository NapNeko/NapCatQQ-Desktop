# -*- coding: utf-8 -*-
# 第三方库导入
from qfluentwidgets import FluentIcon as FIcon
from qfluentwidgets import SplashScreen, MSFluentWindow
from qfluentwidgets.window.fluent_window import NavigationItemPosition
from PySide6.QtCore import QSize
from PySide6.QtWidgets import QApplication

# 项目内模块导入
from src.ui.icon import NCDIcon
from src.ui.resource import resource
from src.ui.home_page import HomePage
from src.core.utils.path import PathFunc
from src.ui.settings_page import SettingsPage
from src.core.utils.singleton import singleton
from src.ui.main_window.title_bar import NCDTitleBar


@singleton
class MainWindow(MSFluentWindow):
    """NapCat Desktop 的主窗体"""

    def __init__(self) -> None:
        """构造函数"""
        super().__init__()
        # 执行路径验证
        PathFunc().path_validator()
        # 调用方法
        self.setWindow()
        self.setItem()
        self._connectSignalToSlot()
        # 组件加载完成结束 SplashScreen
        self.splashScreen.finish()

    def setWindow(self) -> None:
        """设置窗体"""
        # 标题栏部分
        self.titleBar.deleteLater()
        self.tille_bar = NCDTitleBar(self)
        self.setTitleBar(self.tille_bar)

        # 窗体大小
        self.setMinimumSize(1280, 720)

        # 设置窗体居中
        desktop = QApplication.screens()[0].availableGeometry()
        width, height = desktop.width(), desktop.height()
        self.move(width // 2 - self.width() // 2, height // 2 - self.height() // 2)

        # 创建 Splash Screen
        self.splashScreen = SplashScreen(NCDIcon.LOGO.value, self, True)
        self.splashScreen.setIconSize(QSize(360, 260))
        self.splashScreen.raise_()

        # 显示窗体
        self.show()

        # 挂起
        QApplication.processEvents()

    def setItem(self) -> None:
        """设置侧边栏"""

        self.addSubInterface(
            interface=HomePage(),
            icon=FIcon.HOME,
            text=self.tr("主页"),
            position=NavigationItemPosition.TOP,
        )

        self.addSubInterface(
            interface=SettingsPage(),
            icon=FIcon.SETTING,
            text=self.tr("设置"),
            position=NavigationItemPosition.BOTTOM,
        )

    def _connectSignalToSlot(self): ...
