# -*- coding: utf-8 -*-
# 第三方库导入
from qfluentwidgets import SplashScreen, Theme, setTheme
from qfluentwidgets.components.widgets.stacked_widget import FadeEffectAniStackedWidget
from qframelesswindow import FramelessWindow
from PySide6.QtCore import QSize, Qt
from PySide6.QtWidgets import QApplication, QVBoxLayout

# 项目内模块导入
from src.core.utils.singleton import singleton
from src.ui.common.icon import StaticIcon
from src.ui.window.guide_window.ask_page import AskPage
from src.ui.window.guide_window.finsh_page import FinshPage
from src.ui.window.guide_window.install_page import InstallNapCatQQPage, InstallQQPage
from src.ui.window.guide_window.welcome_page import WelcomePage

"""引导用户执行初始化操作模块

Attributes:
    GuideWindow: 引导用户执行初始化操作窗体
"""


@singleton
class GuideWindow(FramelessWindow):
    """引导用户执行初始化操作窗体

    Attributes:
        view (FadeEffectAniStackedWidget): 页面切换组件
        welcome_page (WelcomePage): 欢迎页面
        ask_page (AskPage): 询问页面
        install_qq_page (InstallQQPage): 安装 QQ 页面
        install_nc_page (InstallNapCatQQPage): 安装 NapCatQQ 页面
        finish_page (FinshPage): 完成页面
        vBoxLayout (QVBoxLayout): 布局管理器
        splashScreen (SplashScreen): 启动画面
    """

    def __init__(self) -> None:
        """初始化"""
        super().__init__()

    def initialize(self) -> None:
        """初始化方法

        调整窗体大小、位置、布局等
        """
        # 设置主题
        setTheme(Theme.AUTO)

        # 创建 Splash Screen
        self.splashScreen = SplashScreen(StaticIcon.NAPCAT.path(), self, True)
        self.splashScreen.setIconSize(QSize(360, 260))
        self.splashScreen.raise_()
        self.show()

        # 设置窗体图标
        self.setWindowIcon(StaticIcon.NAPCAT.qicon())

        # 设置窗体大小以及设置打开时居中
        self.setFixedSize(600, 450)
        desktop = QApplication.screens()[0].availableGeometry()
        width, height = desktop.width(), desktop.height()
        self.move(width // 2 - self.width() // 2, height // 2 - self.height() // 2)

        # 隐藏无用的按钮
        self.titleBar.maxBtn.hide()

        # 挂起
        QApplication.processEvents()

        # 调用方法
        self.create_page()
        self.create_layout()

        # 组件加载完成结束 SplashScreen
        self.splashScreen.finish()

    def create_page(self) -> None:
        """创建页面

        页面顺序：欢迎页面 -> 询问页面 -> 安装 QQ 页面 -> 安装 NapCatQQ 页面 -> 完成页面
        通过 FadeEffectAniStackedWidget 实现页面切换时的淡入淡出效果
        通过 on_next_page 方法实现页面的切换
        """
        self.view = FadeEffectAniStackedWidget(self)
        self.welcome_page = WelcomePage(self)
        self.ask_page = AskPage(self)
        self.install_qq_page = InstallQQPage(self)
        self.install_nc_page = InstallNapCatQQPage(self)
        self.finish_page = FinshPage(self)

        self.view.addWidget(self.welcome_page)
        self.view.addWidget(self.ask_page)
        self.view.addWidget(self.install_qq_page)
        self.view.addWidget(self.install_nc_page)
        self.view.addWidget(self.finish_page)

        self.view.setCurrentWidget(self.welcome_page)

    def create_layout(self) -> None:
        """创建布局

        使用 QVBoxLayout 布局管理器将 FadeEffectAniStackedWidget 添加到窗体中
        """
        self.vBoxLayout = QVBoxLayout()
        self.vBoxLayout.setContentsMargins(0, 0, 0, 0)
        self.vBoxLayout.setSpacing(0)
        self.vBoxLayout.addWidget(self.view)
        self.setLayout(self.vBoxLayout)

    def close(self) -> None:
        """关闭窗体

        关闭窗体时将窗体设置为不可见，并打开主窗体
        """

        # 关闭时将窗体设置为不可见
        self.hide()

        # 项目内模块导入
        from src.ui.window.main_window import MainWindow

        MainWindow().initialize()

        super().close()

    def mousePressEvent(self, event):
        """重写鼠标按下事件"""
        if event.button() == Qt.MouseButton.LeftButton:
            self._drag_pos = event.globalPosition().toPoint() - self.pos()
            event.accept()

    def mouseMoveEvent(self, event):
        """重写鼠标移动事件"""
        if event.buttons() == Qt.MouseButton.LeftButton and self._drag_pos is not None:
            self.move(event.globalPosition().toPoint() - self._drag_pos)
            event.accept()

    def mouseReleaseEvent(self, event):
        """重写鼠标释放事件"""
        if event.button() == Qt.MouseButton.LeftButton:
            self._drag_pos = None
            event.accept()

    def on_next_page(self) -> None:
        """切换到下一个页面

        页面顺序：欢迎页面 -> 询问页面 -> 安装 QQ 页面 -> 安装 NapCatQQ 页面 -> 完成页面
        """
        if isinstance(self.view.currentWidget(), WelcomePage):
            self.view.setCurrentWidget(self.ask_page)
        elif isinstance(self.view.currentWidget(), AskPage):
            self.view.setCurrentWidget(self.install_qq_page)
        elif isinstance(self.view.currentWidget(), InstallQQPage):
            self.view.setCurrentWidget(self.install_nc_page)
        elif isinstance(self.view.currentWidget(), InstallNapCatQQPage):
            self.view.setCurrentWidget(self.finish_page)
