# -*- coding: utf-8 -*-
# 第三方库导入
from qfluentwidgets import SplashScreen
from qframelesswindow import FramelessWindow
from qfluentwidgets.components.widgets.stacked_widget import FadeEffectAniStackedWidget
from PySide6.QtCore import Qt, QSize
from PySide6.QtWidgets import QVBoxLayout, QApplication

# 项目内模块导入
from src.core.utils.singleton import singleton
from src.ui.window.guide_window.ask_page import AskPage
from src.ui.window.guide_window.finsh_page import FinshPage
from src.ui.window.guide_window.install_page import InstallQQPage, InstallNapCatQQPage
from src.ui.window.guide_window.welcome_page import WelcomePage


@singleton
class GuideWindow(FramelessWindow):

    def __init__(self) -> None:
        super().__init__()

    def initialize(self) -> None:
        """初始化窗体设置"""
        # 创建 Splash Screen
        self.splashScreen = SplashScreen(":Global/image/Global/napcat.png", self, True)
        self.splashScreen.setIconSize(QSize(360, 260))
        self.splashScreen.raise_()
        self.show()

        # 设置窗体大小以及设置打开时居中
        self.setFixedSize(600, 450)
        desktop = QApplication.screens()[0].availableGeometry()
        width, height = desktop.width(), desktop.height()
        self.move(width // 2 - self.width() // 2, height // 2 - self.height() // 2)

        # 隐藏无用的按钮
        self.titleBar.maxBtn.hide()
        self.titleBar.minBtn.hide()
        self.titleBar.closeBtn.hide()

        # 挂起
        QApplication.processEvents()

        # 调用方法
        self.create_page()
        self.create_layout()

        # 组件加载完成结束 SplashScreen
        self.splashScreen.finish()

    def create_page(self) -> None:
        """创建页面等"""
        self.view = FadeEffectAniStackedWidget(self)
        self.welcomePage = WelcomePage(self)
        self.askPage = AskPage(self)
        self.installQQPage = InstallQQPage(self)
        self.installNapCatQQPage = InstallNapCatQQPage(self)
        self.finshPage = FinshPage(self)

        self.view.addWidget(self.welcomePage)
        self.view.addWidget(self.askPage)
        self.view.addWidget(self.installQQPage)
        self.view.addWidget(self.installNapCatQQPage)
        self.view.addWidget(self.finshPage)

        self.view.setCurrentWidget(self.welcomePage)

    def create_layout(self) -> None:
        """创建布局"""
        self.vBoxLayout = QVBoxLayout()
        self.vBoxLayout.setContentsMargins(0, 0, 0, 0)
        self.vBoxLayout.setSpacing(0)
        self.vBoxLayout.addWidget(self.view)
        self.setLayout(self.vBoxLayout)

    def on_next_page(self) -> None:
        """下一页"""
        if isinstance(self.view.currentWidget(), WelcomePage):
            self.view.setCurrentWidget(self.askPage)
        elif isinstance(self.view.currentWidget(), AskPage):
            self.view.setCurrentWidget(self.installQQPage)
        elif isinstance(self.view.currentWidget(), InstallQQPage):
            self.view.setCurrentWidget(self.installNapCatQQPage)
        elif isinstance(self.view.currentWidget(), InstallNapCatQQPage):
            self.view.setCurrentWidget(self.finshPage)

    def close(self) -> None:
        """重写关闭事件"""

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
