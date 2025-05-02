# -*- coding: utf-8 -*-
# 第三方库导入
from qframelesswindow import FramelessWindow
from qfluentwidgets.components.widgets.stacked_widget import FadeEffectAniStackedWidget
from PySide6.QtWidgets import QVBoxLayout, QApplication

# 项目内模块导入
from src.Core.Utils.singleton import singleton
from src.Ui.GuideWindow.ask_page import AskPage
from src.Ui.GuideWindow.welcome_page import WelcomePage


@singleton
class GuideWindow(FramelessWindow):

    def __init__(self) -> None:
        super().__init__()

    def initialize(self) -> None:
        """初始化窗体设置"""
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

        # 调用方法
        self.create_page()
        self.create_layout()

    def create_page(self) -> None:
        """创建页面等"""
        self.view = FadeEffectAniStackedWidget(self)
        self.welcomePage = WelcomePage(self)
        self.askPage = AskPage(self)

        self.view.addWidget(self.welcomePage)
        self.view.addWidget(self.askPage)

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
            self.view.setCurrentWidget(self.welcomePage)

    def on_back_page(self) -> None:
        """上一页"""
        if isinstance(self.view.currentWidget(), WelcomePage):
            self.view.setCurrentWidget(self.askPage)
        elif isinstance(self.view.currentWidget(), AskPage):
            self.view.setCurrentWidget(self.welcomePage)
