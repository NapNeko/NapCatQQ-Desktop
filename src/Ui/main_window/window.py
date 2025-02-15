# -*- coding: utf-8 -*-
# 第三方库导入
from qfluentwidgets import SplashScreen, MSFluentWindow
from PySide6.QtGui import QPixmap
from PySide6.QtCore import QSize
from PySide6.QtWidgets import QApplication

# 项目内模块导入
from src.ui.icon import NCDIcon
from src.ui.resource import resource
from src.core.utils.path import PathFunc
from src.core.utils.singleton import singleton


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
        # 组件加载完成结束 SplashScreen
        self.splashScreen.finish()

    def setWindow(self) -> None:
        """
        设置窗体
        """
        # 标题栏部分
        self.setWindowIcon(QPixmap(NCDIcon.LOGO.value))
        # 窗体大小以及设置打开时居中
        self.setMinimumSize(1280, 720)
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
        # 添加子页面
